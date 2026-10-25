# FastMoji

[![CI](https://github.com/evandro777/FastMoji/actions/workflows/ci.yml/badge.svg)](https://github.com/evandro777/FastMoji/actions/workflows/ci.yml)
[![Flatpak](https://github.com/evandro777/FastMoji/actions/workflows/flatpak.yml/badge.svg)](https://github.com/evandro777/FastMoji/actions/workflows/flatpak.yml)
[![AppImage](https://github.com/evandro777/FastMoji/actions/workflows/appimage.yml/badge.svg)](https://github.com/evandro777/FastMoji/actions/workflows/appimage.yml)
[![made-for-linux](https://img.shields.io/badge/Linux-1f425f?logo=linux)](https://www.linux.org/)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-orange)](https://opensource.org/licenses/GPL-3.0)

A popup emoji picker for Linux that is **light and fast**. Written in Rust with [Slint](https://slint.dev).

FastMoji is inspired by [Emote](https://github.com/tom-james-watson/Emote) by Tom Watson, a great GTK4 picker. FastMoji keeps the same idea (press a shortcut, search, pick, and the emoji is pasted into the app you were using) and focuses on startup time and memory:

|                                               | Emote 4.0.2 (Python + GTK) | FastMoji       |
|-----------------------------------------------|----------------------------|----------------|
| Idle memory (resident)                        | ~99 MB                     | **~23 MB**     |
| Memory with the picker shown                  | ~99 MB                     | **~27 MB**     |
| Shortcut to window mapped                     | median 130 ms              | **6–12 ms**    |
| Per-keystroke search                          | not measured               | **0.2–0.3 ms** |
| Launch to window, not resident, native binary | not measured               | **~57 ms**     |
| Launch to window, not resident, Flatpak       | not measured               | **~0.5 s**     |

The Flatpak build matches the native one when resident (23 MB idle, 27 MB shown, about 6 ms to open). The "not resident" rows are the *Keep FastMoji in memory* option turned off: FastMoji itself needs about 55 ms to start, and about 450 ms of the Flatpak figure is Flatpak setting up its sandbox, so Flatpak users who care about speed should leave the option on. Measured on one machine (Cinnamon, X11).

![FastMoji searching for hearts](docs/screenshots/search.png)

## Features

- Instant search, 9 categories that follow scrolling, and your recent emojis first
- Keyboard first: arrows (also while typing), `Enter` copy and paste, `Shift+Enter` build a multi-emoji selection, `Ctrl+F` search, `Ctrl+Tab` categories (or `Up` from the top row to reach the category bar, then `Left`/`Right`, `Down` to go back), `Esc` close
- Click copies and pastes, right click adds to the selection
- Skin tone, emoji size and automatic paste preferences
- Global shortcut (default `Ctrl+Alt+.`) on X11
- Optional: **do not stay in memory** (see below)
- Follows the desktop's light/dark theme
- Translated into Português (Brasil), Français, Deutsch, Español, Italiano, 日本語 and 中文: the interface and the emoji names and search keywords (CLDR data, accent-insensitive, English keywords keep working). It follows the system locale; change it in Preferences

## Status

Early pilot. Works on **X11** (tested on Cinnamon): global shortcut, clipboard and automatic paste. On **Wayland** it shows the picker and copies to the clipboard; there is no global shortcut or automatic paste yet (bind a key to `fastmoji --toggle`).

## Install

### Flatpak: easy install with automatic updates

Until FastMoji is on Flathub, every release is also published as a Flatpak remote, so `flatpak update` (or your software centre) keeps it current. A bundle downloaded from Releases does **not** update by itself; you would install the next one over it.

```bash
flatpak remote-add --user --if-not-exists fastmoji https://evandro777.github.io/FastMoji/fastmoji.flatpakrepo
flatpak install --user fastmoji io.github.evandro777.FastMoji
flatpak update --user
```

The remote is not GPG-signed, so it is only as trustworthy as the GitHub repository it is built from. It needs the Flathub remote for the runtime (`flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo`).

### AppImage (with delta updates)

Download `FastMoji-<version>-x86_64.AppImage`, then `chmod +x` it and run it. It uses the fontconfig, xkbcommon and X11 libraries already on your system. For a keyboard shortcut use the command `/path/to/FastMoji-<version>-x86_64.AppImage --toggle` (it must point at the file, so keep it in a fixed place).

It has update information built in, so a tool such as [AppImageUpdate](https://github.com/AppImageCommunity/AppImageUpdate) (`appimageupdatetool FastMoji-*.AppImage`), Gear Lever or AppManager downloads only the changed parts of the next release. The AppImage does not update itself.

## Build & Configurations

[Build and configurations information](BUILD.md)

## Contributing and releases

See [`CONTRIBUTING.md`](CONTRIBUTING.md) to build, test and send a change, and [`RELEASING.md`](RELEASING.md) for how a release is cut. CI (format, clippy, tests, packaging checks, and a Flatpak build) runs on every pull request.

## License

GPL-3.0-or-later (see `LICENSE`). Emoji data, the Noto font and libraries are covered in [`THIRD_PARTY.md`](THIRD_PARTY.md). Thanks to Tom Watson and the Emote contributors for the idea.
