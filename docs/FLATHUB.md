# Publishing on Flathub

Notes for the maintainer. Flathub reviews each new app, and its rules matter here:

## Rules that affect this project (checked against docs.flathub.org/docs/for-app-authors/requirements)

- **App id.** Projects hosted on GitHub must use `io.github.<user>.<App>`; a `com.` id needs a domain you own and can verify. This project uses `io.github.evandro777.FastMoji`, and the id must equal the `<id>` in the metainfo.
- **AI policy.** Submitters must disclose AI-generated code, documentation or packaging. **Flathub manifests must not contain AI-generated or AI-assisted content**, and AI tools or agents must not open or automate the submission pull request. This project's code, docs and the in-repo `flatpak/` files were written with an AI assistant, so:
    - **Write the Flathub manifest yourself**, by hand (you may read `flatpak/io.github.evandro777.FastMoji.yml` to understand it, but do not paste it).
    - **Open the submission PR yourself**, and disclose the AI assistance for the code and docs in it. Read the current policy first; it may change.
- **No network during the build.** Dependencies must be declared as sources. For Rust that means a `cargo-sources.json` produced by [`flatpak-cargo-generator.py`](https://github.com/flatpak/flatpak-builder-tools/tree/master/cargo) from `Cargo.lock`, referenced from the manifest (the in-repo manifest vendors crates with `cargo vendor` instead, which Flathub will not take).
- **Permissions as small as possible.** FastMoji only needs the display sockets and IPC. Say why if asked about `--socket=fallback-x11`.
- **Metainfo must validate** (`appstreamcli validate`), with screenshots served over HTTPS (they come from `raw.githubusercontent.com/.../main/docs/screenshots/`), a license, release notes and OARS content rating. Icon: the SVG in `flatpak/`.
- The submission goes to [flathub/flathub](https://github.com/flathub/flathub) as a pull request against the `new-pr` branch; after approval the app gets its own `flathub/io.github.evandro777.FastMoji` repo, and updates are PRs there.

## Checklist before submitting

- [ ] Repository is public with a release tag and the metainfo URLs resolve (homepage, bugtracker, screenshots).
- [ ] The Flatpak builds and runs from a clean machine (`flatpak/build.sh --devel`), including hotkey, paste and the single-instance socket inside the sandbox. **Not yet verified.**
- [ ] `cargo-sources.json` generated and builds offline.
- [ ] Hand-written Flathub manifest, with the AI disclosure in the PR text.
- [ ] Decide what to say about Wayland: global shortcut and auto-paste are X11 only today.

## Until then

GitHub Releases with the `.flatpak` bundle (built by CI) is the distribution channel.
