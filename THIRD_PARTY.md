# Third-party material

FastMoji is inspired by [Emote](https://github.com/tom-james-watson/Emote) by Tom Watson (GPL-3.0-or-later). It is a separate Rust implementation; the following assets are reused:

| Asset                                                           | Source                                                         | License      |
|-----------------------------------------------------------------|----------------------------------------------------------------|--------------|
| `crates/core/assets/emojis.csv` (emoji list, names, keywords)               | [OpenMoji](https://openmoji.org) data, as shipped with Emote   | CC BY-SA 4.0 |
| `crates/app/assets/NotoColorEmoji.ttf` (emoji glyphs, v2.057 / Unicode 18) | [Google Noto Emoji](https://github.com/googlefonts/noto-emoji) | SIL OFL 1.1  |

Libraries: [Slint](https://slint.dev) (used under GPL-3.0), x11rb, rustybuzz, ttf-parser, png, arboard, serde, toml, raw-window-handle. See `Cargo.lock` for the exact versions.

The application icon (`packaging/io.github.evandro777.FastMoji.svg`) is original artwork for this project.
