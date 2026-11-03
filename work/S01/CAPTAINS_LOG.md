# Captain's log — lcars

**Stardate 2026.10.27, engineering workstation, Linux 7.0.0, 24 cores.**

Mission: a terminal command display that is unmistakably LCARS-inspired and
still an honest system monitor. The design went first, as ordered.

**What was built.** A Rust binary, two dependencies (`crossterm` for raw
mode and input events, `libc` for OS calls), about 5,200 lines including
tests. The command view is a left rail with top and bottom elbows, a
two-row top bar and a one-row bottom bar, rail blocks that carry uptime,
memory, load and disk as labels and fill like gauges, a block-glyph clock as
the focal readout, a CPU readout with a braille trace, and paired sweeps for
memory/disk and RX/TX, with the lower third left black. The diagnostic view
mirrors the frame to the right and shows OS/arch, a per-core bank, swap,
sampling stats and the five busiest processes; it is only reachable via `p`
or `Tab`. A help overlay lists the controls, the active modes and any
collector warnings.

**What was verified.**

* 63 automated tests: rate math (elapsed-time based, zero gaps, hour gaps,
  resets, 32-bit wraps), `/proc` parsers on fixtures including a hostile
  process name, per-interface network tracking with interfaces coming and
  going, process ranking with vanishing PIDs, layout at sizes from 0x0 to
  250x80, ASCII-only output in ASCII mode, unavailable states, negative-space
  and asymmetry measurements on the 120x36 frame, and diff-renderer minimality.
* `clippy -D warnings` and `rustfmt` clean on Linux and on the
  `aarch64-apple-darwin` target; the macOS collector type-checks but was not
  executed, because no Mac was available.
* Live in a 120x36 tmux pane with true colour: every key, the hold pill,
  the help overlay, a resize to 80x24 and back, `q`, `Ctrl-C` and `SIGTERM`
  all leave the shell with `icanon` and `echo` restored and exit code 0.
  Redirected stdout and bad arguments produce actionable errors.
* Measured: 670 KB executable, 0.4–0.5 ms from process start to the first
  flushed frame, 0% CPU over 30 s idle, 2.8 MB resident, about 2.5 KB of
  terminal output per second.
* The 120x36 captures were rendered to PNG and checked against the seven
  visual acceptance questions; all answered yes.

**What was left.** macOS is compiled, not flown. Only the root filesystem is
reported. Wide-character width is a heuristic. True colour is detected from
the environment rather than probed.

Engineering reports the display is stable at warp. End log.
