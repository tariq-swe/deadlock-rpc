use crate::log;
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

/// Called at startup before anything else. If a newer release exists the user
/// is prompted. If they accept, the update is downloaded, applied, and the
/// process is replaced (Linux: exec, Windows: PowerShell swap + exit).
/// Any error is logged and startup continues normally.
pub fn check_on_startup() {
    if let Err(e) = try_check() {
        log!("[updater] Check failed: {e}");
        notify("Update failed — check logs for details.");
    }
}

fn try_check() -> Result<(), Box<dyn std::error::Error>> {
    log!("[updater] Checking for updates (current: v{CURRENT_VERSION})");

    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("deadlock-rpc/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(8))
        .build()?;

    let release: Release = client.get(RELEASES_API).send()?.json()?;
    log!("[updater] Latest release: {}", release.tag_name);

    if !is_newer(&release.tag_name) {
        log!("[updater] Already on latest version");
        return Ok(());
    }

    // Ask the user before downloading anything.
    #[cfg(unix)]
    if !prompt_update_linux(release.tag_name.trim_start_matches('v')) {
        log!("[updater] User skipped update");
        return Ok(());
    }

    #[cfg(windows)]
    if !prompt_update_windows(release.tag_name.trim_start_matches('v')) {
        log!("[updater] User skipped update");
        return Ok(());
    }

    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name())
        .ok_or("release asset not found for this platform")?;

    log!("[updater] Downloading {}", asset.browser_download_url);
    notify("Downloading and installing update, launching shortly...");

    let zip_bytes = client.get(&asset.browser_download_url).send()?.bytes()?;
    log!("[updater] Downloaded {} bytes, extracting...", zip_bytes.len());

    let new_binary = extract_binary(&zip_bytes)?;
    log!("[updater] Extracted binary ({} bytes), writing to disk...", new_binary.len());

    let exe_path = std::env::current_exe()?;

    // Write and rename complete before apply_update returns — only then do we
    // notify the user and exec/restart, so this fires only on full success.
    apply_update(&exe_path, &new_binary)?;
    Ok(())
}

// ── Platform-specific prompt ──────────────────────────────────────────────────

/// Blocking Yes/No dialog on Linux — tries zenity (GTK/GNOME) then kdialog (KDE).
/// Returns true if the user chose to update.
#[cfg(unix)]
fn prompt_update_linux(new_version: &str) -> bool {
    let text = format!(
        "v{new_version} is available (you have v{CURRENT_VERSION}).\nDownload and install now?"
    );

    // zenity: exit 0 = OK pressed
    let zenity = std::process::Command::new("zenity")
        .args(["--question", "--title=Deadlock RPC Update"])
        .arg(format!("--text={text}"))
        .args(["--ok-label=Update Now", "--cancel-label=Skip"])
        .status();

    if let Ok(status) = zenity {
        return status.success();
    }

    // kdialog: exit 0 = Yes pressed
    std::process::Command::new("kdialog")
        .args(["--title", "Deadlock RPC Update", "--yesno"])
        .arg(&text)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Shows the update dialog with a fake version — for local dev testing only.
#[cfg(unix)]
pub fn show_update_prompt_dev() {
    prompt_update_linux("99.0.0");
}

#[cfg(windows)]
pub fn show_update_prompt_dev() {
    prompt_update_windows("99.0.0");
}

/// Shows a Yes/No message box via the Windows API directly.
#[cfg(windows)]
fn prompt_update_windows(new_version: &str) -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::winuser::{MessageBoxW, IDYES, MB_ICONQUESTION, MB_YESNO};

    let message = format!(
        "v{new_version} is available (you have v{CURRENT_VERSION}).\r\nDownload and install now?"
    );
    let message_wide: Vec<u16> = OsStr::new(&message)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let caption_wide: Vec<u16> = OsStr::new("Deadlock RPC Update")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message_wide.as_ptr(),
            caption_wide.as_ptr(),
            MB_YESNO | MB_ICONQUESTION,
        )
    };

    result == IDYES
}

// ── Platform-specific apply ───────────────────────────────────────────────────

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

    log!("[updater] Binary replaced, restarting via exec...");

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

    log!("[updater] Binary staged, PowerShell swap in progress — exiting");
    std::process::exit(0);
}
