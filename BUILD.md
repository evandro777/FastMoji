# Build

## How it is built

- **Rust + Slint** with the winit backend and the **software renderer** (no GPU/GL context, which is where most of the memory of a toolkit app goes).
- **Emoji drawn as images**: the software renderer cannot draw the bitmap (CBDT) Noto font, so `src/glyphs.rs` shapes each emoji (`rustybuzz`), reads its PNG from the font (`ttf-parser`, `png`) and downsizes it to the exact pixel size it is shown at, so it stays sharp. Only the visible rows are decoded.
- **Emoji data baked in** by `build.rs` from `assets/emojis.csv`; search is a substring match over precomputed lowercase terms.
- **X11 through `x11rb`**: `XGrabKey` hotkey, XTEST Ctrl+V paste (Ctrl+Shift+V when the previous window is a known terminal), EWMH focus hand-over, and a small clipboard owner that knows when another app takes the clipboard.
- **Single instance** through a Unix socket; the daemon shows/hides one window.

## First build

Install Rust toolchain and dependencies to build.

Debian/Ubuntu/Mint:

```bash
sudo apt install libfontconfig-dev libxkbcommon-dev libxkbcommon-x11-dev libx11-dev libxcb1-dev rustup`
rustup default stable
```

## How to run

```bash
cargo run --release              # start the daemon and show the picker
cargo build --profile fast       # quick iteration build (no LTO)
cargo test
```

| Command                 | Effect                                                                      |
|-------------------------|-----------------------------------------------------------------------------|
| `fastmoji`              | Show the picker (starts the daemon if needed)                               |
| `fastmoji --toggle`     | Show/hide it. Bind this to a key on Wayland, or use it as your own shortcut |
| `fastmoji --background` | Start without showing a window (autostart)                                  |

## From a release (Flatpak bundle)

Download `FastMoji-<version>-x86_64.flatpak` from the [Releases](https://github.com/evandro777/FastMoji/releases) page, then:

```bash
flatpak install --user ./FastMoji-<version>-x86_64.flatpak
flatpak run io.github.evandro777.FastMoji
```

## Build the Flatpak yourself

App id: `io.github.evandro777.FastMoji`.

```bash
sudo apt install flatpak-builder
flatpak install --user flathub org.freedesktop.Sdk//25.08 org.freedesktop.Sdk.Extension.rust-stable//25.08
flatpak/build.sh --devel      # installs io.github.evandro777.FastMoji.Devel
flatpak run io.github.evandro777.FastMoji.Devel
```

`flatpak/build.sh` without `--devel` builds the real id; `--bundle` also writes `flatpak/fastmoji.flatpak`. Crates are vendored into `flatpak/vendor` (about 700 MB, git-ignored) because the sandboxed build has no network. The sandbox only gets the X11/Wayland sockets. Settings live in `~/.var/app/<app-id>/data/FastMoji/settings.toml`. For a desktop shortcut that runs the Flatpak use the command `flatpak run io.github.evandro777.FastMoji`.

## Configuration

`~/.local/share/FastMoji/settings.toml` (written by the Preferences panel):

```toml
shortcut = "Ctrl+Alt+."   # X11 global shortcut, also set in Preferences by clicking the button and pressing the keys (e.g. "Ctrl+Alt+E", "Super+F2", "Ctrl+Alt+Tab"; keys: letters, digits, F1-F12, Tab, Return, space, period, comma, minus)
keep_resident = true      # see below
auto_paste = true
skin_tone = 0             # 0 = default, 1..5
emoji_size = 28           # 20, 24, 28, 32 or 36
language = ""             # "" = system locale, "en", "pt_BR", "fr", "de", "es", "it", "ja" or "zh_CN"
```

### Keeping FastMoji in memory

By default FastMoji stays resident, so it opens instantly and the global shortcut works. In *Preferences → Keep FastMoji in memory* (or `keep_resident = false`) you can turn that off:

- FastMoji exits as soon as the picker is closed, so **the global shortcut no longer works**. Open it with your own desktop shortcut that runs `fastmoji` (for example Super+Y in Cinnamon's keyboard settings). This is still very fast: you can skip the built-in shortcut entirely, bind `fastmoji` in your desktop environment's keyboard settings and leave "Keep FastMoji in memory" off, so nothing stays in RAM between uses.
- After you pick an emoji and it is pasted, FastMoji exits at once. The clipboard then belongs to nobody, so the emoji stays available only if a clipboard manager (CopyQ, ...) keeps it. If nothing was pasted (automatic paste off), FastMoji waits for *your* paste: it exits shortly after another application reads the clipboard (clipboard managers reading it in the first second are ignored), as soon as the clipboard is replaced, or after 5 minutes.
- On Wayland the setting is locked on, because the clipboard needs the process.

### Restarting after a crash

Release builds abort on a panic, so a bug can never leave a half-working process behind, but then the global shortcut stays dead until FastMoji is started again. To have it restarted automatically, run it as a systemd user service instead of a plain autostart entry (`~/.config/systemd/user/fastmoji.service`, then `systemctl --user enable --now fastmoji`):

```ini
[Unit]
Description=FastMoji emoji picker

[Service]
ExecStart=%h/.cargo/bin/fastmoji --background
Restart=on-failure
RestartSec=1

[Install]
WantedBy=graphical-session.target
```

Adjust `ExecStart` to where `fastmoji` is installed (for the Flatpak use `flatpak run io.github.evandro777.FastMoji --background`). Quitting from Preferences is a normal exit and is not restarted. Run with `FASTMOJI_DEBUG=1` or read `journalctl --user -u fastmoji` to see the panic message.
