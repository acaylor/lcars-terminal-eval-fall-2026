# Captain's log

Stardate 2026-09-22. Workstation `linux`, Linux 6.8.0-139-generic, x86_64, rustc 1.98.1.

Built a Rust terminal engineering display with an asymmetric LCARS-style frame: upper-left elbow, segmented spine, right-hand staircase of telemetry pills, and a bottom rail that does not meet the left edge. The focal element is a 9-row amber clock. Process rows stay on the secondary view.

Verified on this machine:

- `cargo test --locked --offline` — 14 library tests and 9 layout tests passed.
- `cargo clippy --locked --all-targets -- -D warnings` — clean.
- `cargo check --locked --target x86_64-apple-darwin --target aarch64-apple-darwin` — clean. Not executed on macOS.
- `cargo build --locked --release --offline` — `target/release/lcars-monitor`, 483656 bytes.
- `./scripts/measure.sh` — `startup_us=238` (0.238 ms), `/usr/bin/time` elapsed 0.00 s, RSS 2200–2284 KiB, HWM 2284 KiB, idle CPU 0.00% over 4 seconds (below one clock tick).
- A pty session accepted `?`, `p`, space, and `q`, then exited 0. `Ctrl-C` exited 0 and restored the terminal. A redirected stdout exited 2 with an actionable error.
- Primary 120×36 silhouette is 66.7% empty. Secondary is 65.9% empty. The demo process name is absent from the primary frame and present on the diagnostic frame.

The clock, not a grid of boxes, is the thing you see first. That was the point.
