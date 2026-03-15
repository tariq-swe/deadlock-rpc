use crate::log;
#[cfg(not(debug_assertions))]
use std::path::PathBuf;

const DEADLOCK_APP_ID: &str = "1422450";

pub fn launch_deadlock() {
    log!("[launcher] Launching Deadlock with -condebug...");
    match launch_via_steam() {
        Ok(_) => log!("[launcher] Steam launch initiated."),
        Err(e) => log!("[launcher] Failed to launch Deadlock: {e}"),
    }
}

#[cfg(unix)]
fn launch_via_steam() -> std::io::Result<()> {
    std::process::Command::new("steam")
        .args(["-applaunch", DEADLOCK_APP_ID, "-condebug"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(windows)]
fn launch_via_steam() -> std::io::Result<()> {
    let steam_exe = crate::steam::steam_exe_path()
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Program Files (x86)\Steam\steam.exe"));
    std::process::Command::new(steam_exe)
        .args(["-applaunch", DEADLOCK_APP_ID, "-condebug"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(not(debug_assertions))]
pub fn install_shortcut() {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            log!("[install] Could not determine executable path: {e}");
            return;
        }
    };

    match install_platform_shortcut(&exe) {
        Ok(dest) => log!("[install] Shortcut created: {}", dest.display()),
        Err(e) => log!("[install] Failed to create shortcut: {e}"),
    }
}

#[cfg(all(unix, not(debug_assertions)))]
fn install_platform_shortcut(exe: &std::path::Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let apps_dir = dirs::home_dir()
        .ok_or("could not find home directory")?
        .join(".local/share/applications");

    std::fs::create_dir_all(&apps_dir)?;

    let desktop = format!(
        "[Desktop Entry]\n\
         Version=1.0\n\
         Name=Deadlock RPC\n\
         Comment=Discord Rich Presence for Deadlock\n\
         Exec={exe}\n\
         Terminal=false\n\
         Type=Application\n\
         Categories=Game;\n",
        exe = exe.display()
    );

    let dest = apps_dir.join("deadlock-rpc.desktop");
    std::fs::write(&dest, desktop)?;

    // Make executable
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;

    Ok(dest)
}

#[cfg(all(windows, not(debug_assertions)))]
fn install_platform_shortcut(exe: &std::path::Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let desktop = dirs::desktop_dir().ok_or("could not find Desktop directory")?;
    let lnk = desktop.join("Deadlock RPC.lnk");

    // Use PowerShell to create a proper .lnk shortcut
    let script = format!(
        r#"$s=(New-Object -COM WScript.Shell).CreateShortcut('{lnk}');$s.TargetPath='{exe}';$s.Save()"#,
        lnk = lnk.display(),
        exe = exe.display(),
    );

    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()?;

    if !status.success() {
        return Err("PowerShell shortcut creation failed".into());
    }

    Ok(lnk)
}
