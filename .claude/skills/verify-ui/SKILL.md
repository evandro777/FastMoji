---
name: verify-ui
description: Build FastCleany, run it, and look at the changed Slint page in a window screenshot before calling a UI change done. Use after any change to crates/app/ui or the Rust that feeds it.
---

# Verify a FastCleany UI change

Do not commit, push or create branches. Report at the end; never claim a UI change works from a passing build alone.

1. **Checks:** `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test && cargo audit`. Stop and fix on any failure.
2. **Launch:** `cargo build --profile fast`, then start `target/fast/fastcleany` in the background and remember its PID (kill by PID later, never `pkill -f`).
3. **Find the window:** `xwininfo -name FastCleany` gives the window id. Wait until it exists.
4. **Navigate:** open the page you changed. The sidebar order is Startup, Services, Boot, Apps, Leftovers, Storage, Disk health, History. Click by finding the button from a window screenshot and the window's own geometry (`xwininfo`), never hard-coded screen coordinates. If you cannot drive the click safely, say so and ask the user to open the page.
5. **Screenshot the window only:** `import -window <id> <scratchpad>/page.png`, then Read the PNG. Never capture the whole screen.
6. **Inspect:**
   - rows do not stretch when a list has one item
   - badges and buttons are aligned, nothing is cut off at the 400 px minimum width
   - text that should be copyable is selectable as one block
   - the empty state ("nothing found") and an error state do not throw or look broken
   - the new strings exist in `pt_BR` (`dev/gen_po.py`, then regenerate)
7. **Clean up:** kill the app by PID.
8. **Report:** a short list of what was verified in the running app, what was verified only by tests, and what the user must test by hand. Anything behind `pkexec` (SMART, uninstall, GRUB, systemctl changes, Trash/Docker actions) always goes in the manual list.
