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

    let path = match shortcut_path(&exe) {
        Some(p) => p,
        None => {
            log!("[install] Could not determine shortcut path");
            return;
        }
    };

    if path.exists() {
        log!("[install] Shortcut already exists, skipping");
        return;
    }

    if !prompt_shortcut() {
        log!("[install] User declined shortcut creation");
        return;
    }

    match install_platform_shortcut(&exe) {
        Ok(dest) => log!("[install] Shortcut created: {}", dest.display()),
        Err(e) => log!("[install] Failed to create shortcut: {e}"),
    }
}

#[cfg(all(unix, not(debug_assertions)))]
fn shortcut_path(exe: &std::path::Path) -> Option<PathBuf> {
    Some(exe.parent()?.join("deadlock-rpc.desktop"))
}

#[cfg(all(windows, not(debug_assertions)))]
fn shortcut_path(exe: &std::path::Path) -> Option<PathBuf> {
    Some(exe.parent()?.join("Deadlock RPC.lnk"))
}

#[cfg(all(unix, not(debug_assertions)))]
fn prompt_shortcut() -> bool {
    let mut accepted = false;
    let Ok(handle) = notify_rust::Notification::new()
        .appname("Deadlock RPC")
        .summary("Create Shortcut?")
        .body("Would you like to create a shortcut in the install folder?")
        .action("yes", "Yes")
        .action("no", "No")
        .show()
    else {
        // If we can't show a notification, default to creating the shortcut.
        return true;
    };
    handle.wait_for_action(|action| {
        accepted = action == "yes";
    });
    accepted
}

#[cfg(all(windows, not(debug_assertions)))]
fn prompt_shortcut() -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::winuser::{MessageBoxW, IDYES, MB_ICONQUESTION, MB_YESNO};

    let message_wide: Vec<u16> = OsStr::new(
        "Would you like to create a shortcut in the install folder?",
    )
    .encode_wide()
    .chain(std::iter::once(0))
    .collect();
    let caption_wide: Vec<u16> = OsStr::new("Deadlock RPC")
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

#[cfg(not(debug_assertions))]
fn icon_path(exe: &std::path::Path, filename: &str) -> Option<PathBuf> {
    let p = exe.parent()?.join("assets").join(filename);
    if p.exists() { Some(p) } else { None }
}

#[cfg(all(unix, not(debug_assertions)))]
fn install_platform_shortcut(exe: &std::path::Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = exe.parent().ok_or("could not determine executable directory")?;

    let icon_line = icon_path(exe, "icon.png")
        .map(|p| format!("Icon={}\n", p.display()))
        .unwrap_or_default();

    let desktop = format!(
        "[Desktop Entry]\n\
         Version=1.0\n\
         Name=Deadlock RPC\n\
         Comment=Discord Rich Presence for Deadlock\n\
         Exec={exe}\n\
         {icon_line}\
         Terminal=false\n\
         Type=Application\n\
         Categories=Game;\n",
        exe = exe.display()
    );

    let dest = dir.join("deadlock-rpc.desktop");
    std::fs::write(&dest, desktop)?;

    // Make executable
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;

    Ok(dest)
}

#[cfg(all(windows, not(debug_assertions)))]
fn install_platform_shortcut(exe: &std::path::Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = exe.parent().ok_or("could not determine executable directory")?;
    let lnk = dir.join("Deadlock RPC.lnk");

    let icon_part = icon_path(exe, "icon.ico")
        .map(|p| format!("$s.IconLocation='{}';", p.display()))
        .unwrap_or_default();

    // Use PowerShell to create a proper .lnk shortcut
    let script = format!(
        r#"$s=(New-Object -COM WScript.Shell).CreateShortcut('{lnk}');$s.TargetPath='{exe}';{icon_part}$s.Save()"#,
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
