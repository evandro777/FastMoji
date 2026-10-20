# Releasing

Releases are Git tags. CI builds the Flatpak bundle and attaches it to the GitHub release.

1. Pick the version (`0.2.0`). Update **both**:
   - `Cargo.toml` → `version`
   - `flatpak/io.github.evandro777.FastMoji.metainfo.xml` → add a `<release version="0.2.0" date="YYYY-MM-DD">` entry with notes (newest first)

2. Run `cargo update -p fastmoji` if `Cargo.lock` changed, then `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.

3. Merge to `master` through a pull request, wait for CI.

4. Tag and push:

   ```bash
   git tag -s v0.2.0 -m "FastMoji 0.2.0"    # or without -s
   git push origin v0.2.0
   ```

5. The `Flatpak` workflow checks that the tag equals the `Cargo.toml` version and that the metainfo has the `<release>` entry, builds the bundle, and publishes `FastMoji-0.2.0-x86_64.flatpak` plus its `.sha256` on the release page.

6. Smoke-test the downloaded bundle: `flatpak install --user ./FastMoji-0.2.0-x86_64.flatpak`, press the shortcut, paste an emoji.

Flathub: see <https://docs.flathub.org/docs/for-app-authors/requirements>.
