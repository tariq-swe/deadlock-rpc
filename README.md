# Deadlock RPC

Discord Rich Presence for Deadlock — automatically shows your current hero, game state, and match mode on your Discord profile in real time.

> **Not affiliated with Valve Corporation or the Deadlock development team.**

---

## Preview

![Deadlock RPC in action](assets/preview.png)

---

## Features

- **Hero display** — shows your current hero's name and card image
- **Game state tracking** — Hideout, In Queue, Match Intro, In Match, Post Match, Spectating
- **Match mode detection** — Standard, Street Brawl, Bot Match, Training Range, and more
- **Hero-specific hideout messages** — unique status text per hero while in the Hideout
- **Live elapsed timer** — tracks how long you have been in-session
- **System tray icon** — runs quietly in the system tray with a Quit option on both Windows and Linux
- **Auto-updater** — checks for new releases on startup and prompts you to install them
- **Auto-launch** — launches Deadlock with the required flag automatically
- **Self-installing** — creates a desktop shortcut on first run
- **Auto-exit** — closes itself when you close Deadlock
- **Fully customizable** — presence text, timer, hero display, poll rate, and more via `config.toml`

---

## How It Works

Deadlock RPC launches the game with the `-condebug` flag, which causes Deadlock to write its internal console output to a log file. The app monitors this file in real time, parsing log lines to detect hero selection, map loads, phase transitions, and match mode. State changes are pushed to Discord via its IPC protocol.

No game memory is read, no files are modified, and no network traffic is intercepted — the app is entirely read-only with respect to the game.

---

## Installation

**Requirements:** Discord must be running.

1. Go to the [Releases](../../releases) page
2. Download and extract the zip for your platform:
   - **Windows:** `deadlock-rpc-setup-windows-x86_64.zip`
   - **Linux:** `deadlock-rpc-setup-linux-x86_64.zip`
3. Run the binary inside the extracted folder:
   - **Windows:** double-click `deadlock-rpc.exe`
   - **Linux:** `chmod +x deadlock-rpc && ./deadlock-rpc`
4. A desktop shortcut named **Deadlock RPC** is created automatically
5. Deadlock launches with Rich Presence active

From this point forward, use the **Deadlock RPC** shortcut instead of launching Deadlock directly.

> **Keep the extracted folder intact.** Logs are written to the `logs/` folder inside it.

### Flags

| Flag | Description |
|------|-------------|
| `--no-launch` | Start the monitor without launching Deadlock |

### Windows SmartScreen

Windows may show a **"Windows protected your PC"** warning on first run. This is because the executable is unsigned, not because it contains malware. Click **More info → Run anyway** to proceed, or [build from source](#building-from-source) to verify the binary yourself.

---

## Auto-updater

On startup, Deadlock RPC checks GitHub for a newer release.

- **Linux** — a notification appears with **Update Now / Skip** action buttons
- **Windows** — a dialog box appears with **Yes / No** options

If you accept, the update is downloaded, applied, and the app restarts automatically.

---

## Customization

On first run a **`config.toml`** is created next to the executable with all options documented. Edit it with any text editor — changes take effect on the next launch. Any key you omit falls back to its default.

### General

| Key | Default | Description |
|-----|---------|-------------|
| `general.auto_launch` | `true` | Launch Deadlock on startup. |
| `general.auto_exit` | `true` | Exit when the game closes. |
| `general.launch_timeout_s` | `120` | Seconds to wait for the game to appear after launch. |
| `general.log_poll_interval_ms` | `500` | How often (ms) to check the game log. Lower = faster updates. |
| `general.presence_update_interval_s` | `5` | How often (seconds) to refresh the Discord presence card. |

### Presence

| Key | Default | Description |
|-----|---------|-------------|
| `presence.show_elapsed_timer` | `true` | Show the elapsed time counter. |
| `presence.show_hero` | `true` | Show the hero image and name. |
| `presence.details_with_hero` | `"Playing as {hero}"` | Top line when a hero is known. |
| `presence.details_no_hero` | `"{phase}"` | Top line when no hero is known. |

### Per-phase status strings

| Key | Default |
|-----|---------|
| `presence.status.not_running` | `"Not Running"` |
| `presence.status.main_menu` | `"Browsing the Main Menu"` |
| `presence.status.in_hideout` | `"In the Hideout"` |
| `presence.status.in_queue` | `"Searching for a Match..."` |
| `presence.status.match_intro` | `"{mode} • Loading into Match"` |
| `presence.status.in_match` | `"{mode} • Battling in {location}"` |
| `presence.status.in_match_location` | `"the Cursed Apple"` |
| `presence.status.post_match` | `"Reviewing Match Results"` |
| `presence.status.spectating` | `"Watching a Match"` |

### Images

| Key | Default | Description |
|-----|---------|-------------|
| `images.default_large_image` | `"deadlock_logo"` | Large image asset when no hero is shown. |
| `images.default_large_text` | `"Deadlock"` | Tooltip for the large image. |
| `images.small_image` | `"deadlock_logo"` | Small corner overlay image asset. |
| `images.small_text` | `"Deadlock"` | Tooltip for the small image. |

### Template variables

| Variable | Available in | Value |
|----------|-------------|-------|
| `{hero}` | `details_with_hero`, `in_hideout` | Hero display name, e.g. `Vindicta` |
| `{phase}` | `details_no_hero` | Phase label, e.g. `Post Match` |
| `{mode}` | `match_intro`, `in_match` | Match mode, e.g. `Standard Match` |
| `{location}` | `in_match` | Value of `in_match_location` |

### Examples

```toml
# Minimal presence — no hero name, no timer
[presence]
show_elapsed_timer = false
details_with_hero  = "Playing Deadlock"
details_no_hero    = "Playing Deadlock"

# Custom in-match status
[presence.status]
in_match = "Grinding {mode}"
in_queue = "Waiting for a game..."

# Keep the app open after the game closes
[general]
auto_exit = false
```

---

## Manual `-condebug` setup

If you prefer to manage Deadlock's launch options yourself, add `-condebug` to Steam's launch options for Deadlock (**Library → right-click Deadlock → Properties → General → Launch Options**), then run Deadlock RPC with `--no-launch`.

---

## Building from Source

Requires [Rust](https://rustup.rs) stable.

```bash
git clone https://github.com/tariq-swe/deadlock-rpc.git
cd deadlock-rpc
cargo build --release
./target/release/deadlock-rpc
```

---

## Contributing

Contributions are welcome. Open an issue first for non-trivial changes. Keep PRs focused — one feature or fix each. Format with `cargo fmt`, lint with `cargo clippy`. Bug reports with the contents of `logs/deadlock-rpc.log` are especially helpful.

---

## Disclaimer

Not affiliated with, endorsed by, or connected to Valve Corporation. **Deadlock**, all hero names, images, and related assets are the property of **Valve Corporation**. Hero images displayed in Discord are sourced from the community-maintained [Deadlock API](https://deadlock-api.com) and remain the property of Valve. This project does not distribute or claim ownership of any Valve assets.
