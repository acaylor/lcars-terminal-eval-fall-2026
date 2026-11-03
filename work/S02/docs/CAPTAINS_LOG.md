# Captain's log

**Stardate 2026.264 — Engineering, main console.**

Starfleet asked for a command display, not another `top`. I chose Rust: a
native binary with no runtime, direct access to `/proc` on Linux and to Mach,
sysctl and libproc on macOS, and full control over every byte sent to the
terminal. The dependency manifest has two entries, `libc` and
`unicode-width`, and no TUI framework.

The console is drawn the way a designer would draw it. Elbows, rails and
sidebar blocks are painted into a sub-pixel raster at twice the terminal's
resolution, and labels are cut into the solid colour afterwards. The clock
uses a small stroke font rasterized by distance field, so its numerals stay
condensed and rounded at any size. A concentric sweep gauge carries CPU,
memory and disk. A Braille trace follows whichever subsystem Tab selects. A
mirrored, cool-toned diagnostic deck behind `p` holds the per-core comb and
the top five processes. That deck is the only place processes appear, and
the table isn't even read until it opens.

Telemetry is kept honest. Rates come from measured intervals, counters that
wrap or reset are recognized, and anything unavailable says so (`N/A`,
`DENIED`, `SYNC`) instead of inventing a number.

Verified on this Linux workstation:

- 77 automated tests pass, and clippy is clean on Linux and both macOS
  targets.
- All eight pty lifecycle drills pass. Quit, Esc, Ctrl-C, SIGINT, SIGTERM,
  SIGHUP, a resize storm down to 1×1, and Ctrl-Z/fg each leave the terminal
  as they found it.
- The first frame arrives in about 2 ms.
- Idle cost is 0.18 % of one core and 3.1 MiB resident.
- The 120×36 console is 54 % negative space and passes all seven visual
  acceptance questions.

One item stays open. The macOS collector compiles and lints clean but hasn't
flown yet. Its first live test on a Mac is logged as the next away mission.

End log.
