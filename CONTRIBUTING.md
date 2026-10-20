# Contributing

Thanks for helping. FastMoji's goals are **light and fast** and **simple**: a change that adds memory, start-up time or a heavy dependency needs a measurement and a reason.

## Workflow (GitHub flow)

1. Fork or branch from `main`, make a focused change.
2. Run the checks locally (same as CI):
   ```bash
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```
3. Open a pull request. CI must be green (it also builds the Flatpak). Describe what changed and, for UI or X11 behaviour, how you tested it.
4. `main` stays releasable; releases are tags (see `RELEASING.md`).

## Building

System packages (Debian/Ubuntu/Mint): `libfontconfig-dev libxkbcommon-dev libxkbcommon-x11-dev libx11-dev libxcb1-dev`, plus a Rust toolchain.

```bash
cargo build --profile fast     # quick, no LTO
cargo run --profile fast
FASTMOJI_DEBUG=1 cargo run --profile fast   # decision log on stderr
```

## Testing the desktop behaviour

Unit tests cover the data and settings logic. The X11 behaviour (hotkey, focus, paste) is checked manually or with the scripts in `dev/` (see `AGENTS.md`). If you change focus, clipboard or paste code, run `dev/t_paste.py` many times: failures there were intermittent before they were fixed.

## Style

- Rust 2024, `rustfmt` with `max_width = 120` (see `rustfmt.toml`), no clippy warnings.
- Comments explain *why*. Keep modules small; avoid abstractions without a second use.
- User-visible behaviour changes update `README.md`; new settings get a default and a unit test that old files still load.

## AI-assisted contributions

AI tools are welcome as helpers, but you are responsible for what you submit: understand it, test it, and say so in the pull request if a substantial part was generated.
