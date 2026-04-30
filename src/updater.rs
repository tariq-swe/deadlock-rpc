use log::{debug, info, warn};
use std::io::{Cursor, Read};

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const RELEASES_API: &str =
    "https://api.github.com/repos/tariq-swe/deadlock-rpc/releases/latest";

#[derive(serde::Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(serde::Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

fn is_newer(tag: &str) -> bool {
    let tag = tag.trim_start_matches('v');
    let parse = |s: &str| -> (u32, u32, u32) {
        let mut p = s.splitn(3, '.');
        let n = |p: &mut std::str::SplitN<'_, char>| {
            p.next().and_then(|x| x.parse().ok()).unwrap_or(0)
        };
        (n(&mut p), n(&mut p), n(&mut p))
    };
    parse(tag) > parse(CURRENT_VERSION)
}

fn verify_sha256(data: &[u8], digest: &str) -> Result<(), Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};
    let expected = digest
        .strip_prefix("sha256:")
        .ok_or("asset digest has unexpected format (expected \"sha256:<hex>\")")?;
    let actual = format!("{:x}", Sha256::digest(data));
    if actual != expected {
        return Err(format!("SHA-256 mismatch — expected {expected}, got {actual}").into());
    }
    Ok(())
}

#[cfg(windows)]
fn asset_name() -> &'static str {
    "deadlock-rpc-setup-windows-x86_64.zip"
}
#[cfg(not(windows))]
fn asset_name() -> &'static str {
    "deadlock-rpc-setup-linux-x86_64.zip"
}

#[cfg(windows)]
fn zip_binary_path() -> &'static str {
    "deadlock-rpc/deadlock-rpc.exe"
}
#[cfg(not(windows))]
fn zip_binary_path() -> &'static str {
    "deadlock-rpc/deadlock-rpc"
}

fn extract_binary(zip_bytes: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    let mut file = archive.by_name(zip_binary_path())?;
    let mut out = Vec::new();
    file.read_to_end(&mut out)?;
    Ok(out)
}

#[cfg(not(windows))]
fn notify(body: &str) {
    let _ = notify_rust::Notification::new()
        .appname("Deadlock RPC")
        .summary("Deadlock RPC")
        .body(body)
        .show();
}

#[cfg(windows)]
fn notify(_body: &str) {}

// Debug-only: simulates an update prompt for v99.9.9, fake-downloads, then
// re-execs the binary without `--simulate-update` to mimic a post-update launch.
#[cfg(debug_assertions)]
pub fn simulate_update() {
    const FAKE_VERSION: &str = "99.9.9";
    debug!("[updater] Simulating update to v{FAKE_VERSION}");

    #[cfg(unix)]
    if !prompt_update_linux(FAKE_VERSION) {
        debug!("[updater] User skipped simulated update");
        return;
    }
    #[cfg(windows)]
    if !prompt_update_windows(FAKE_VERSION) {
        debug!("[updater] User skipped simulated update");
        return;
    }

    debug!("[updater] Simulating download (3s)...");
    notify("Downloading and installing update, launching shortly...");
    std::thread::sleep(std::time::Duration::from_secs(3));
    debug!("[updater] Simulated download complete, restarting...");

    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            debug!("[updater] Failed to get exe path: {e}");
            return;
        }
    };
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a != "--simulate-update")
        .collect();

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(&exe_path).args(&args).exec();
        debug!("[updater] exec failed: {err}");
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new(&exe_path).args(&args).spawn();
        std::process::exit(0);
    }
}

// Called at startup before anything else. If a newer release exists the user
// is prompted. If they accept, the update is downloaded, applied, and the
// process is replaced (Linux: exec, Windows: PowerShell swap + exit).
// Any error is logged and startup continues normally.
pub fn check_on_startup() {
    if let Err(e) = try_check() {
        warn!("[updater] Check failed: {e}");
        notify("Update failed — check logs for details.");
    }
}

fn try_check() -> Result<(), Box<dyn std::error::Error>> {
    info!("[updater] Checking for updates (current: v{CURRENT_VERSION})");

    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("deadlock-rpc/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(8))
        .build()?;

    let release: Release = client.get(RELEASES_API).send()?.json()?;
    info!("[updater] Latest release: {}", release.tag_name);

    if !is_newer(&release.tag_name) {
        info!("[updater] Already on latest version");
        return Ok(());
    }

    // Ask the user before downloading anything.
    #[cfg(unix)]
    if !prompt_update_linux(release.tag_name.trim_start_matches('v')) {
        info!("[updater] User skipped update");
        return Ok(());
    }

    #[cfg(windows)]
    if !prompt_update_windows(release.tag_name.trim_start_matches('v')) {
        info!("[updater] User skipped update");
        return Ok(());
    }

    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name())
        .ok_or("release asset not found for this platform")?;

    info!("[updater] Downloading {}", asset.browser_download_url);

    let zip_bytes = client.get(&asset.browser_download_url).send()?.bytes()?;
    info!("[updater] Downloaded {} bytes, verifying checksum...", zip_bytes.len());

    let digest = asset
        .digest
        .as_deref()
        .ok_or("release asset has no digest; refusing to install unverified update")?;
    verify_sha256(&zip_bytes, digest)?;
    info!("[updater] Checksum verified ok");
    notify("Downloading and installing update, launching shortly...");

    let new_binary = extract_binary(&zip_bytes)?;
    info!("[updater] Extracted binary ({} bytes), writing to disk...", new_binary.len());

    let exe_path = std::env::current_exe()?;

    // Write and rename complete before apply_update returns — only then do we
    // notify the user and exec/restart, so this fires only on full success.
    apply_update(&exe_path, &new_binary)?;
    Ok(())
}

const CHANGELOG_URL: &str = "https://github.com/tariq-swe/deadlock-rpc/releases/latest";

// Blocking Yes/No dialog on Linux — tries zenity (GTK/GNOME) then kdialog (KDE).
// Loops if the user clicks "View Changelog" (opens browser, then re-shows the prompt).
// Returns true if the user chose to update.
#[cfg(unix)]
fn prompt_update_linux(new_version: &str) -> bool {
    let text = format!(
        "v{new_version} is available (you have v{CURRENT_VERSION}).\nDownload and install now?"
    );

    loop {
        let zenity_out = std::process::Command::new("zenity")
            .args(["--question", "--title=Deadlock RPC Update"])
            .arg(format!("--text={text}"))
            .args([
                "--ok-label=Update Now",
                "--cancel-label=Skip",
                "--extra-button=View Changelog",
            ])
            .output();

        if let Ok(out) = zenity_out {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.trim() == "View Changelog" {
                let _ = std::process::Command::new("xdg-open").arg(CHANGELOG_URL).spawn();
                continue;
            }
            return out.status.success();
        }

        let kdialog_status = std::process::Command::new("kdialog")
            .args([
                "--title", "Deadlock RPC Update",
                "--yesnocancel", &text,
                "--yes-label", "Update Now",
                "--no-label", "Skip",
                "--cancel-label", "View Changelog",
            ])
            .status();

        match kdialog_status.as_ref().ok().and_then(|s| s.code()) {
            Some(0) => return true,
            Some(1) => return false,
            Some(2) => {
                let _ = std::process::Command::new("xdg-open").arg(CHANGELOG_URL).spawn();
                continue;
            }
            _ => return false,
        }
    }
}

// Shows a Yes/No/Cancel message box via the Windows API.
// Yes = Update Now, No = Skip, Cancel = View Changelog (opens browser, re-shows dialog).
#[cfg(windows)]
fn prompt_update_windows(new_version: &str) -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::winuser::{MessageBoxW, IDCANCEL, IDNO, IDYES, MB_ICONQUESTION, MB_YESNOCANCEL};

    let to_wide = |s: &str| -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    };

    let message = format!(
        "v{new_version} is available (you have v{CURRENT_VERSION}).\r\n\r\n\
        Yes \u{2014} Update Now\r\n\
        No \u{2014} Skip\r\n\
        Cancel \u{2014} View Changelog"
    );
    let caption = to_wide("Deadlock RPC Update");

    loop {
        let result = unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                to_wide(&message).as_ptr(),
                caption.as_ptr(),
                MB_YESNOCANCEL | MB_ICONQUESTION,
            )
        };

        match result {
            IDYES => return true,
            IDNO => return false,
            IDCANCEL => {
                let _ = std::process::Command::new("cmd")
                    .args(["/c", "start", "", CHANGELOG_URL])
                    .spawn();
                // re-show the dialog after the browser opens
            }
            _ => return false,
        }
    }
}

#[cfg(unix)]
fn apply_update(
    exe_path: &std::path::Path,
    new_binary: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::CommandExt;

    let tmp = exe_path.with_extension("new");
    std::fs::write(&tmp, new_binary)?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    std::fs::rename(&tmp, exe_path)?;

    info!("[updater] Binary replaced, restarting via exec...");

    // exec() replaces the current process image with the new binary.
    // Because we haven't acquired the single-instance port yet, the new
    // binary starts fresh with no lock conflicts.
    let err = std::process::Command::new(exe_path)
        .args(std::env::args().skip(1))
        .exec();

    Err(err.into())
}

#[cfg(windows)]
fn apply_update(
    exe_path: &std::path::Path,
    new_binary: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let tmp = exe_path.with_file_name("deadlock-rpc.new.exe");
    std::fs::write(&tmp, new_binary)?;

    // PowerShell waits for this process to exit, then swaps the binary and starts it.
    // We haven't bound the single-instance port yet so the new process starts cleanly.
    let script = format!(
        "Start-Sleep 2; Move-Item -Force '{tmp}' '{exe}'; Start-Process '{exe}'",
        tmp = tmp.display(),
        exe = exe_path.display(),
    );

    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .spawn()?;

    info!("[updater] Binary staged, PowerShell swap in progress — exiting");
    std::process::exit(0);
}
