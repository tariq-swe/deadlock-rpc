use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Load the icon PNG from the assets directory, returning raw RGBA bytes.
/// Checks next to the executable first (release), then the current working
/// directory (development via `cargo run`).
fn load_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let candidates = [
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("assets").join("icon.png"))),
        Some(std::path::PathBuf::from("assets/icon.png")),
    ];

    let path = candidates
        .into_iter()
        .flatten()
        .find(|p| p.exists())?;

    let img = image::open(path).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    Some((img.into_raw(), w, h))
}

/// Shows a "Launching Deadlock..." tray icon with a right-click Quit option.
/// Blocks until `game_launched` is set, then removes the icon and returns.
pub fn run_launching_tray(game_launched: Arc<AtomicBool>) {
    #[cfg(target_os = "linux")]
    linux::run(game_launched);

    #[cfg(not(target_os = "linux"))]
    windows::run(game_launched);
}

// ── Linux ─────────────────────────────────────────────────────────────────────
// Uses ksni: a pure-Rust StatusNotifierItem implementation over D-Bus.
// Works natively on KDE Plasma and any SNI-compatible desktop. No GTK needed.

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::thread;
    use std::time::Duration;

    struct DeadlockTray {
        // ARGB32 (network byte order) pixel data for the icon
        argb: Vec<u8>,
        icon_w: i32,
        icon_h: i32,
    }

    impl ksni::Tray for DeadlockTray {
        fn id(&self) -> String {
            "deadlock-rpc".to_string()
        }

        fn title(&self) -> String {
            "Deadlock RPC".to_string()
        }

        fn icon_name(&self) -> String {
            // Fallback system icon if no pixmap is available
            "applications-games".to_string()
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            if self.argb.is_empty() {
                return vec![];
            }
            vec![ksni::Icon {
                width: self.icon_w,
                height: self.icon_h,
                data: self.argb.clone(),
            }]
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: "Deadlock RPC".to_string(),
                icon_name: String::new(),
                icon_pixmap: vec![],
                description: String::new(),
            }
        }

        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::*;
            vec![StandardItem {
                label: "Quit".to_string(),
                activate: Box::new(|_| std::process::exit(0)),
                ..Default::default()
            }
            .into()]
        }
    }

    fn build_tray() -> DeadlockTray {
        let (argb, icon_w, icon_h) = match super::load_rgba() {
            Some((rgba, w, h)) => {
                // SNI requires ARGB32 in network (big-endian) byte order.
                // image crate gives RGBA, so reorder: [A, R, G, B].
                let argb: Vec<u8> = rgba
                    .chunks_exact(4)
                    .flat_map(|p| [p[3], p[0], p[1], p[2]])
                    .collect();
                (argb, w as i32, h as i32)
            }
            None => (vec![], 0, 0),
        };
        DeadlockTray { argb, icon_w, icon_h }
    }

    pub fn run(game_launched: Arc<AtomicBool>) {
        // Spawns the SNI service in its own thread. The icon persists for the
        // lifetime of the process, giving the user a right-click Quit at all times.
        ksni::TrayService::new(build_tray()).spawn();

        while !game_launched.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(100));
        }
    }
}

// ── Windows ───────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "linux"))]
mod windows {
    use super::*;
    use std::thread;
    use std::time::Duration;
    use tray_icon::{Icon, TrayIconBuilder};

    fn load_icon() -> Icon {
        if let Some((rgba, w, h)) = super::load_rgba() {
            if let Ok(icon) = Icon::from_rgba(rgba, w, h) {
                return icon;
            }
        }
        // Fallback: small blue square
        let size = 32u32;
        let rgba: Vec<u8> = (0..(size * size))
            .flat_map(|_| [40u8, 120u8, 200u8, 255u8])
            .collect();
        Icon::from_rgba(rgba, size, size).expect("fallback icon failed")
    }

    pub fn run(game_launched: Arc<AtomicBool>) {
        use tray_icon::menu::{Menu, MenuItem};

        let menu = Menu::new();
        let quit_item = MenuItem::new("Quit", true, None);
        menu.append(&quit_item).unwrap();

        let tray = TrayIconBuilder::new()
            .with_tooltip("Deadlock RPC")
            .with_icon(load_icon())
            .with_menu(Box::new(menu))
            .build()
            .expect("Failed to create tray icon");

        loop {
            // Any menu event at this stage can only be Quit
            if tray_icon::menu::MenuEvent::receiver().try_recv().is_ok() {
                std::process::exit(0);
            }
            if game_launched.load(Ordering::Relaxed) {
                let _ = tray.set_visible(false);
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
}
