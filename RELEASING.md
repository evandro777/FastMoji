# Releasing

Releases are Git tags. CI builds the Flatpak bundle and attaches it to the GitHub release.

1. Pick the version (`0.2.0`). Update **both**:
    - [Cargo.toml](./Cargo.toml) → `version`
    - [io.github.evandro777.FastMoji.metainfo.xml](./flatpak/io.github.evandro777.FastMoji.metainfo.xml) → add a `<release version="0.2.0" date="YYYY-MM-DD">` entry with notes (newest first)

2. Run `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo audit`
    - It will update `Cargo.lock` follows the new version (CI uses `--locked` and fails on a stale lock). Commit both.
3. Stage the files, commit and push, and Merge to `master` through a pull request, wait for CI.
4. Tag and push (the tag must equal the version, or `appimage.yml` and `flatpak.yml` fail):

    ```bash
    git tag v0.2.0 -m "FastMoji 0.2.0"    # signed add parameter: -s
    git push origin v0.2.0
    ```

5. The `Flatpak` workflow checks that the tag equals the `Cargo.toml` version and that the metainfo has the `<release>` entry, builds the bundle, and publishes `*-x86_64.flatpak` on the release page.
    - The `AppImage` workflow adds `*-x86_64.AppImage`, and its `.zsync` (delta updates) to the same release (it waits up to 30 min for the release to exist).
6. Smoke-test the downloaded AppImage on a clean user, and the Flatpak: `flatpak update` from the previous version should offer the new one.

Flathub: see <https://docs.flathub.org/docs/for-app-authors/requirements>.

## One-time setup

Actions and settings, that needs to be done once.

### Flatpak remote (GitHub Pages)

On a `v*` tag `flatpak.yml` also deploys the OSTree repo and `fastmoji.flatpakrepo` to GitHub Pages. One-time: repository Settings -> Pages -> Source: **GitHub Actions**. Not exercised yet (first tag will be the test).
