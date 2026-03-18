# Deadlock RPC

Discord Rich Presence for Deadlock — automatically shows your current hero, game state, and match mode on your Discord profile in real time.

> **Not affiliated with Valve Corporation or the Deadlock development team.** This is an independent, open-source project.

---

## Preview

![Deadlock RPC in action](assets/preview.png)

---

## Features

- **Hero display** — shows your current hero's name and card image
- **Game state tracking** — Hideout, In Queue, Match Intro, In Match, Post Match, Spectating
- **Match mode detection** — Standard Match, Street Brawl, Bot Match, Training Range, and more
- **Hero-specific hideout messages** — unique status text per hero while in the Hideout
- **Live elapsed timer** — tracks how long you have been in-session
- **Auto-launch** — launches Deadlock with the required flag automatically
- **Self-installing** — creates a desktop shortcut on first run, no extra steps needed
- **Auto-exit** — closes itself when you close Deadlock
- **Fully customizable** — presence text, timer, hero display, poll rate, and more via `config.toml`

---

## How It Works

Deadlock can write its internal console output to a log file when launched with the `-condebug` flag. Deadlock RPC monitors this file in real time, parsing log lines with regex patterns to detect:

- Hero selection and changes
- Map loads and game phase transitions
- Match mode (player count, bot presence, map name)
- Game shutdown and session end

When state changes are detected, the Discord presence is updated via Discord's IPC protocol. No game memory is read, no files are modified, and no network traffic is intercepted — the app is entirely read-only with respect to the game.

---

## Efficiency & FPS Impact

Deadlock RPC is built in **Rust**, chosen specifically for its minimal runtime overhead and zero garbage collection pauses. The application:

- Reads only the **tail of the log file** — it does not load the entire file into memory
- Polls for changes every **500ms** by default (configurable) using standard file I/O, not filesystem watchers
- Updates Discord presence only when **state actually changes** — no redundant IPC calls
- Runs entirely in the **background** with negligible CPU and memory usage
- Has **no impact on game performance or FPS** — it operates independently of the game process

---

## Installation

### Requirements

- **Discord** must be running

### Steps

1. Go to the [Releases](../../releases) page
2. Download and extract the zip for your platform:
   - **Windows:** `deadlock-rpc-setup-windows-x86_64.zip`
   - **Linux:** `deadlock-rpc-setup-linux-x86_64.zip`
3. Run the binary inside the extracted folder once:
   - **Windows:** double-click `deadlock-rpc.exe`
   - **Linux:** `chmod +x deadlock-rpc && ./deadlock-rpc`
4. A desktop shortcut named **Deadlock RPC** is created automatically
5. Deadlock launches immediately with Rich Presence active

From this point forward, use the **Deadlock RPC** shortcut instead of launching Deadlock directly.

> **Keep the extracted folder intact.** Logs are written to the `logs/` folder inside it.

### Flags

| Flag | Description |
|------|-------------|
| `--no-launch` | Start the RPC monitor without launching Deadlock |

---

## ✦ Customization

On first run, Deadlock RPC creates a **`config.toml`** file in the same folder as the executable. Every option is documented inside it. Edit it with any text editor — changes take effect on the next launch.

> **Tip:** You don't need to include every option. Any key you leave out or delete falls back to its default automatically.

### Behavior

| Key | Default | Description |
|-----|---------|-------------|
| `general.auto_launch` | `true` | Launch Deadlock automatically on startup. Set `false` to behave like `--no-launch` every time. |
| `general.auto_exit` | `true` | Exit when the game closes. Set `false` to keep the process running. |
| `general.launch_timeout_s` | `120` | Seconds to wait for the game to appear before giving up. |
| `general.log_poll_interval_ms` | `500` | How often (ms) to check the game log for new events. Lower = faster updates. |
| `general.presence_update_interval_s` | `5` | How often (seconds) to refresh the Discord presence card. |

### Presence display

| Key | Default | Description |
|-----|---------|-------------|
| `presence.show_elapsed_timer` | `true` | Show or hide the elapsed time counter. |
| `presence.show_hero` | `true` | Show the hero image and name. Set `false` to always display the Deadlock logo. |
| `presence.details_with_hero` | `"Playing as {hero}"` | Top line of the presence card when a hero is known. |
| `presence.details_no_hero` | `"{phase}"` | Top line when no hero is known (menus, post-match, etc.). |

### Per-phase status strings

These control the bottom line of the presence card. Edit any or all of them:

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
| `images.default_large_image` | `"deadlock_logo"` | Large image asset key when no hero image is shown. |
| `images.default_large_text` | `"Deadlock"` | Tooltip for the large image when no hero is shown. |
| `images.small_image` | `"deadlock_logo"` | Small corner overlay image asset key. |
| `images.small_text` | `"Deadlock"` | Tooltip for the small corner image. |

### Template variables

Some strings support `{variable}` placeholders that are filled in at runtime:

| Variable | Available in | Value |
|----------|-------------|-------|
| `{hero}` | `details_with_hero`, `in_hideout` | Hero display name, e.g. `Vindicta` |
| `{phase}` | `details_no_hero` | Current phase label, e.g. `Post Match` |
| `{mode}` | `match_intro`, `in_match` | Match mode, e.g. `Standard Match` |
| `{location}` | `in_match` | Value of `in_match_location` |

### Examples

**Minimal presence — no hero name, no timer:**
```toml
[presence]
show_elapsed_timer = false
details_with_hero  = "Playing Deadlock"
details_no_hero    = "Playing Deadlock"
```

**Custom in-match status:**
```toml
[presence.status]
in_match          = "Grinding {mode}"
in_match_location = "the streets"   # unused if you remove {location} from in_match
in_queue          = "Waiting for a game..."
```

**Slower polling for even lower overhead:**
```toml
[general]
log_poll_interval_ms       = 1000
presence_update_interval_s = 10
```

**Keep the app open after the game closes (useful if you restart often):**
```toml
[general]
auto_exit = false
```

---

## Manual Launch Option (`-condebug`)

Deadlock RPC automatically launches Deadlock with the required `-condebug` flag. If you prefer to manage Deadlock's launch options yourself — for example, if you launch through a different shortcut — you can set the flag directly in Steam:

1. Open **Steam** and go to your **Library**
2. Right-click **Deadlock** and select **Properties**
3. Under the **General** tab, find the **Launch Options** field
4. Enter `-condebug` (you can combine it with any existing options, e.g. `-condebug -novid`)
5. Close the Properties window

Once set, you can launch Deadlock normally and then run Deadlock RPC with the `--no-launch` flag to skip the automatic launch:

```
./deadlock-rpc --no-launch
```

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

Contributions are welcome. Please follow these guidelines:

- **Open an issue first** for non-trivial changes to align on approach before writing code
- **Keep PRs focused** — one feature or fix per pull request
- **No breaking changes** to existing CLI flags without discussion
- **Test manually** against a running Deadlock session where possible
- Code is formatted with `cargo fmt` and linted with `cargo clippy`

Bug reports with the contents of `logs/deadlock-rpc.log` are especially helpful for diagnosing state detection issues.

---

## Disclaimer

This project is not affiliated with, endorsed by, or connected to Valve Corporation or the Deadlock development team in any way.

**Deadlock**, the Deadlock logo, all hero names, hero images, and all related in-game assets are the property of **Valve Corporation**. All rights reserved.

Hero images and game data displayed in the Discord presence are sourced from the community-maintained [Deadlock API](https://deadlock-api.com) and are the intellectual property of Valve Corporation. They are used here solely for non-commercial, informational display within Discord Rich Presence and remain the property of their respective owners.

This project does not distribute, modify, or claim ownership of any Valve assets. If you are a rights holder and have concerns, please open an issue and they will be addressed promptly.