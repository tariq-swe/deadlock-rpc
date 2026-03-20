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

    let path = candidates.into_iter().flatten().find(|p| p.exists())?;

    let img = image::open(path).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    Some((img.into_raw(), w, h))
}

/// Spawns the system tray icon and blocks forever.
/// The only way out is the user clicking Quit, which calls process::exit.
pub fn run() {
    #[cfg(target_os = "linux")]
    linux::run();

    #[cfg(not(target_os = "linux"))]
    windows::run();
}

// ── Linux ─────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod linux {
    use std::thread;
    use std::time::Duration;

    struct DeadlockTray {
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
                // image crate gives RGBA, so reorder each pixel: [A, R, G, B].
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

    pub fn run() {
        ksni::TrayService::new(build_tray()).spawn();

        // Keep the main thread alive. The ksni daemon thread owns the icon;
        // Quit exits the process directly.
        loop {
            thread::sleep(Duration::from_secs(60));
        }
    }
}

// ── Windows ───────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "linux"))]
mod windows {
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

    pub fn run() {
        use tray_icon::menu::{Menu, MenuEvent, MenuItem};

        let menu = Menu::new();
        let quit_item = MenuItem::new("Quit", true, None);
        let quit_id = quit_item.id().clone();
        menu.append(&quit_item).unwrap();

        let _tray = TrayIconBuilder::new()
            .with_tooltip("Deadlock RPC")
            .with_icon(load_icon())
            .with_menu(Box::new(menu))
            .build()
            .expect("Failed to create tray icon");

        // Windows requires a Win32 message pump for the tray context menu to
        // appear. Without PeekMessage/DispatchMessage the hidden tray window
        // never processes WM_RBUTTONUP and the menu is never shown.
        unsafe {
            use winapi::um::winuser::{DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE};
            let mut msg: MSG = std::mem::zeroed();
            loop {
                while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                while let Ok(event) = MenuEvent::receiver().try_recv() {
                    if event.id == quit_id {
                        std::process::exit(0);
                    }
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
}
