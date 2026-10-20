# AGENTS.md: FastMoji

Notes for AI agents and contributors. Read this before changing anything.

## What this is

**FastMoji** is a popup emoji picker for Linux, written in Rust with Slint. Its whole point is to be **light and fast** (memory, open time, search), while staying **simple**.

Owner priorities, in order: reliable clipboard and automatic paste; fast open; instant search; low memory; simplicity (a little extra memory is fine if it removes complexity). No pixel parity with libadwaita is wanted: clean built-in Slint (Fluent) styling.

Target desktop: **Cinnamon, X11 first** (full feature set), Cinnamon Wayland second (copy only for now). Distribution: **Flatpak**, app id `io.github.evandro777.FastMoji`.

## Layout

```txt
Cargo.toml / build.rs     build.rs compiles ui/ and bakes assets/emojis.csv into static tables
ui/main.slint             the whole UI (Window, grid, category bar, footer, preferences overlay)
src/main.rs               args (--toggle, --background), forwards to a running instance, backend select
src/app.rs                State, RowsModel (lazy rows), App (interaction, window lifecycle, paste, exit logic)
src/emojis.rs             generated data + search/lookup/skin tones (has unit tests)
src/settings.rs           settings.toml (serde), recents (has unit tests)
src/glyphs.rs             emoji -> slint::Image (rustybuzz + ttf-parser + png)
src/x11.rs                hotkey (XGrabKey), paste (XTEST), focus (EWMH), ClipboardOwner
src/instance.rs           single instance over a Unix socket
flatpak/                  manifest, desktop file, metainfo, icon, build.sh
dev/                      manual test harness (python-xlib), NOT shipped
README.md, CONTRIBUTING.md, RELEASING.md, THIRD_PARTY.md, LICENSE
docs/                     screenshots (used by the metainfo)
.github/                  CI workflows, dependabot
```

## Commands

```bash
cargo build --profile fast    # iteration build: no LTO, incremental (~15 s after the first ~6 min build)
cargo build --release         # thin LTO, ~1-2 min incremental, 29 MB binary (the 10 MB font is embedded)
cargo test                    # unit tests (emojis, settings, shortcut parsing)
FASTMOJI_DEBUG=1 ./target/fast/fastmoji   # logs open/focus/hide/paste/exit decisions to stderr
```

System deps: `libfontconfig-dev libxkbcommon-dev libxkbcommon-x11-dev libx11-dev libxcb1-dev`. No GL/Skia/femtovg is used.

## Architecture decisions (do not undo without measuring)

- **Software renderer + emoji as images.** Slint's GL renderer (femtovg) draws the CBDT bitmap Noto font but costs ~108 MB RSS; the software renderer costs ~27 MB but draws *nothing* for that font. So `glyphs.rs` shapes each emoji, takes its PNG from the font, area-averages it down to the **exact physical size it is shown at** (logical size × window scale factor, cache keyed by emoji and size, up to 600 images). Do not go back to one fixed size scaled by Slint: its software renderer scales bilinearly, which skips source pixels when shrinking more than 2x and made the emoji jagged, worse at some sizes than others. The image sizes in `ui/main.slint` are mirrored as constants in `app.rs`. Backend is forced in `main.rs` (`winit` + `software`) and in `Cargo.toml` features (`default-features = false`).
- **Lazy rows.** `RowsModel` builds each `Line` on demand so only visible rows are decoded. All rows have the same height (titles included). Layout is rebuilt on width/size/search changes; `State::rebuild` is cheap.
- **Never hold a `State` borrow while calling `model.notify.reset()`** or anything that makes Slint re-read the model; drop the `RefCell` borrow first.
- **Resident window, show/hide.** The process stays alive; `open()` shows, `hide()` hides. Window size and recents persist in `settings.toml`.
- **Own X11 clipboard** (`x11::ClipboardOwner`), not `arboard`, because we must know when another app takes the clipboard and when the text was read. `arboard` is only the fallback on non-X11 sessions.
- **`keep_resident` setting** (default true; Preferences switch). When false: no global hotkey, `--background` exits immediately, and the process quits after hiding, except: after a *paste* it waits until the target app has read the clipboard (+ short grace, max 2 s), and in *copy-only* cases it waits for the user's own paste (a read more than 1 s after copying; earlier reads are clipboard managers), a clipboard replacement, or 5 min. Locked on for non-X11 sessions.
- **Single instance** socket: `$XDG_RUNTIME_DIR/fastmoji.sock`, or `$XDG_RUNTIME_DIR/app/$FLATPAK_ID/fastmoji.sock` inside Flatpak (each `flatpak run` is a separate sandbox and only that dir is shared).
- Keep dependencies few and light. No tokio/async runtime; threads only for the X11 hotkey, the clipboard server and the socket listener, and they hand work to the UI thread with `slint::invoke_from_event_loop` (`post()` in `app.rs`).

## Hard-won gotchas (X11 / Muffin)

- **Focus on show:** Muffin ignores a plain `show()`. `x11::activate` stamps `_NET_WM_USER_TIME` with the hotkey event time, calls `XSetInputFocus` and sends `_NET_ACTIVE_WINDOW` with that time. `app.rs` also retries every 100 ms until a focus-in arrives, and ignores focus-out for 300 ms after showing (re-activating instead of hiding).
- **Paste must wait for the previous window.** We record `_NET_ACTIVE_WINDOW` before showing; after hiding we poll until that window is active **and** holds the X input focus, restoring it with a pager-style `_NET_ACTIVE_WINDOW` if the WM did not.
- **XTEST key events must be flushed one by one (~8 ms apart) and followed by a round-trip before the connection closes.** Sending the four events back-to-back and dropping the connection made `xed` (GTK3) lose ~25% of pastes. Fixed; do not "optimize" it.
- **Arrow keys:** Left/Right/Up/Down always move through the emoji (handled in `capture-key-pressed`), even while the search box has text. Up on the top emoji row (no search) sets `categories-focused`: then Left/Right cycle categories and Down/Enter (or any other key) return to the grid. `move-vertical` returns false when there is no row to move to.
- A global key can be owned by one client only; `x11::spawn_hotkey` reports `BadAccess`. The user's other tools may hold shortcuts.
- Slint notes: use `content-y` (not `viewport-y`); a callback named `close` shadows a builtin (we use `dismiss`); `FocusScope.capture-key-pressed` is needed to intercept keys before `LineEdit`.

## Platform status

- **X11 (Cinnamon 6.6):** complete. Hotkey, clipboard, auto-paste, focus, residency option.
- **Cinnamon Wayland:** the portal service only has `Background`; no GlobalShortcuts/RemoteDesktop/Clipboard portals. So: copy to clipboard only, and the user binds `fastmoji --toggle` to a key. `arboard` is built with `wayland-data-control` (without it the Linux backend is X11-only, i.e. XWayland); it falls back to X11 if the compositor lacks the protocol. **Untested on a real Wayland session.** Still to probe on a real Wayland session: which protocols Muffin exposes (virtual keyboard, data-control) to see whether paste/clipboard can be improved.
- **GNOME/KDE:** out of scope for now.

## Testing

- Unit tests: `cargo test`.
- Manual harness in `dev/` uses python-xlib (`import Xlib`) to send synthetic input and to screenshot **only FastMoji's own window** (`find_window` + `shot`; never capture the whole screen, the user's desktop is private). Typical run:

```bash
export XDG_DATA_HOME=$PWD/dev/data XDG_RUNTIME_DIR=$PWD/dev/run   # isolates settings and the socket
# dev/data/FastMoji/settings.toml should set shortcut = "Ctrl+Alt+Y"
nohup xed --standalone dev/scratch.txt &                          # paste target; verify by saving and reading the file
./target/fast/fastmoji --background &
python3 dev/t_paste.py 20        # keyboard path;  `mouse` and `manual` variants exist
python3 dev/t_nonresident.py 15  # needs keep_resident = false in the test settings
python3 dev/t_focus.py 30        # hotkey -> Esc reliability
python3 dev/lat.py y "*" <pid> 12   # open latency + RSS
```

- Synthetic input is swallowed if the screen locks (long builds can trigger it). Occasional paste misses appear when the machine is under heavy CPU load right after a build; re-run before assuming a regression.
- **Destructive keys:** the harness sends Ctrl+A, Delete and Ctrl+S to the scratch `xed`. Always go through `focus_scratch_editor()` in `dev/drive.py`, which aborts unless the active window is the scratch file. (A fixed-position click once hit a file manager showing `src/` and trashed the sources; they were restored from the Trash.) Never click or type at hard-coded screen coordinates.
- **Shell hygiene:** never `pkill -f` with a pattern that also appears in your own command line (it kills the shell); anchor it (`'^\./target/fast/fastmoji'`). Never `pkill -x xed` or `pkill -x fastmoji`: that kills the user's own windows; kill by PID.

## Flatpak

`flatpak/build.sh --devel` builds and installs `io.github.evandro777.FastMoji.Devel` (use `--devel` so an installed release is not replaced). Needs `flatpak-builder` and `org.freedesktop.Sdk//25.08` + `org.freedesktop.Sdk.Extension.rust-stable//25.08`. Crates are vendored by the script (offline build). Sandbox permissions are only the Wayland/X11 sockets and IPC. **The manifest has been validated but not yet built or run**: still to verify are the hotkey, paste, focus and socket sharing inside the sandbox.

## Conventions

- Rust edition 2024, `rustfmt` (max_width 120). Comments explain *why*, not what. Small modules, no unnecessary abstraction.
- Settings are serde TOML with `#[serde(default)]` so older files keep loading; add a unit test for new options.
- Update `README.md` when behaviour or commands change, and this file when you learn something non-obvious.
- Settings path: `$XDG_DATA_HOME/FastMoji/settings.toml` (Flatpak: `~/.var/app/<id>/data/FastMoji/`).

## Public project, GitHub flow and Flathub

- Repo: `https://github.com/evandro777/FastMoji` (this directory is the repo root). Work on branches, PRs to `master`; CI is `.github/workflows/ci.yml` (fmt, clippy `-D warnings`, tests, metainfo/desktop validation) and `flatpak.yml` (bundle on every push/PR; on a `v*` tag it publishes the bundle to the GitHub release). `RELEASING.md` has the exact steps; the tag must equal the `Cargo.toml` version and the metainfo needs a matching `<release>`.
- App id is `io.github.evandro777.FastMoji` because Flathub requires `io.github.*` for GitHub-hosted apps.
- **Flathub policy (important for agents):** Flathub manifests must NOT contain AI-generated or AI-assisted content, AI must not open or automate submission PRs, and AI-generated code/docs must be disclosed by the submitter. So: do **not** write or paste a Flathub manifest, `cargo-sources.json` PR text, or submission PR for the owner. You may explain the process (`docs/FLATHUB.md`), keep the in-repo `flatpak/` files for local/CI builds, and keep the README's AI-assistance note accurate.
- Formatting is enforced: `rustfmt.toml` has `max_width = 120`; clippy must be clean. Let-chains are used, so `rust-version` is 1.92 (Slint 1.18 requires it).

## Open items

Flatpak build verification; autostart (Background portal); About dialog; Wayland probe; first real run of the Flatpak CI workflow (untested until pushed); the owner's hand-written Flathub submission.
