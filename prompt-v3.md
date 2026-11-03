# Programming Challenge: LCARS-Inspired Terminal System Monitor

## Mission briefing

Starfleet needs a fast, polished terminal command display for monitoring an engineering workstation. Build a production-quality terminal application with a distinctive LCARS-inspired interface: asymmetric framing, sweeping rails, rounded-looking terminal shapes, bold color fields, compact labels, and a clear hierarchy of focal readouts.

The result should feel fun and unmistakably inspired by futuristic starship interfaces, while remaining useful as a real system-monitoring tool. This is an aesthetic-first terminal experience, not `top`, `htop`, or `btop` with an LCARS color theme. Create an original interpretation rather than copying a specific copyrighted screen pixel-for-pixel.

You are responsible for choosing the implementation language and architecture.

## Priority order

When requirements compete, preserve this order:

1. A distinctive and coherent primary visual composition
2. Correct terminal lifecycle and smooth rendering
3. Accurate core metrics on macOS and Linux
4. Secondary diagnostics
5. Documentation and final polish

Do not trade primary-view quality for additional features.

## Prime directive

Choose the language you believe will provide the best combination of:

1. Fast startup
2. Low CPU and memory overhead
3. Smooth terminal rendering
4. Reliable macOS and Linux support

Briefly explain your choice before presenting the implementation. Do not choose a language merely because it is convenient for generating code; choose what gives the end user the best result.

## Platform and distribution requirements

The application must:

- Detect and use 24-bit color when available for a richer palette and smoother gradients.
- Treat true color as progressive enhancement: loss of true color may reduce fidelity but must not break layout or functionality.
- Support current macOS and Linux systems.
- Require no network connection after installation.
- Include reproducible build instructions.
- Avoid requiring elevated privileges.
- Handle missing system metrics gracefully.
- Restore terminal state, cursor visibility, and input mode after normal exit or interruption.
- Avoid shelling out to utilities such as `top`, `uptime`, `ps`, `vm_stat`, or `date` on every refresh.
- Clearly document any third-party libraries and why they were selected.

Cross-compilation is welcome, but one identical binary is not expected to run on both macOS and Linux.

## Information architecture

The primary command view must integrate at least:

- Local time, including seconds
- Current date
- Hostname
- System uptime
- Overall CPU utilization
- Load averages
- Memory used, available, and total
- Disk usage for at least the root filesystem
- Network receive and transmit rates

The secondary diagnostic view must at least show:

- Operating system and architecture
- Per-core CPU utilization
- Swap usage, when available
- Process count
- The five processes currently using the most CPU, showing PID, name, CPU usage, and memory usage

Do not fabricate unsupported metrics. Display a tasteful unavailable state when data cannot be collected.

The process list must not appear on the default screen. It does not need filtering, scrolling, multiple sort modes, or process-management actions.

## Visual direction

The default screen must not consist primarily of bordered rectangular widgets, tables, bar charts, or vertically stacked metric rows. A submission that resembles a conventional system monitor with colored boxes has missed the central design goal.

Build the composition around these characteristics:

- A dominant asymmetric silhouette
- Thick horizontal and vertical color rails
- Pill-shaped, elbow-shaped, or sweeping control regions approximated with terminal glyphs
- Large focal readouts for time, load, and system status
- Strong hierarchy between primary status, secondary telemetry, and decoration
- Deliberate negative space
- Short alphanumeric labels integrated into the frame
- Animated status indicators and changing data traces
- A restrained palette of warm orange, amber, salmon, lavender, and cool blue
- At least two visually distinct views rather than one screen containing everything

Use Unicode half-block, quadrant, box-drawing, and Braille characters where they improve apparent resolution. Do not rely exclusively on full block characters. Rounded shapes may be approximated using Unicode glyphs, colored spaces, and careful use of terminal background colors.

The visual structure should remain recognizable if all text is removed. Color alone is not sufficient to establish the LCARS-inspired identity.

## Layout and rendering requirements

The interface must:

- Remain readable without sacrificing the composition to maximum information density.
- Refresh smoothly without visibly clearing and repainting the entire terminal.
- Adapt to terminal resizing.
- Present a deliberately simplified command display at `80x24` instead of squeezing every metric onto the screen.
- Reveal additional information and richer composition on larger terminals.
- Avoid corrupted layouts and panics at extremely small sizes.
- Offer a monochrome or reduced-color mode.
- Provide an ASCII-safe degraded fallback when Unicode rendering is disabled.
- Show rates using elapsed time between samples rather than assuming an exact refresh interval.
- Keep idle resource usage reasonably low.

Do not let the ASCII fallback, the smallest supported terminal, or secondary diagnostic views dictate the design of the primary experience.

## Interaction requirements

Implement at least these controls:

- `q` or `Esc`: quit
- `?`: help overlay
- Space: pause/resume updates
- `p`: open or close the process diagnostic view
- `Tab`: cycle focus or command views

Interactions should be discoverable from the interface.

## Visual acceptance test

Before considering the implementation complete, capture the primary interface at `120x36` and assess it against these questions:

1. Is the overall silhouette asymmetric?
2. Is there one immediately dominant focal element?
3. Is at least 20 percent of the screen intentional negative space?
4. Are metrics integrated into the composition rather than placed in interchangeable dashboard boxes?
5. Would the layout remain recognizable if all text were replaced by blanks?
6. Does it look materially different from `top`, `htop`, and `btop`?
7. Does the process list remain absent until the user opens the secondary view?

If any answer is no, revise the visual design before finishing. Include the captured `120x36` primary view and one secondary view in the deliverables.

## Robustness requirements

The program must correctly handle:

- Terminal resize events
- `Ctrl-C` and termination signals
- Narrow or short terminals
- Long hostnames and process names
- Non-ASCII process names
- Counter rollover or reset
- Processes disappearing during sampling
- Network interfaces appearing or disappearing
- Zero or unusually long time between samples
- Unsupported metrics
- Permission-denied errors
- Redirected output or an unsuitable terminal

Do not allow one failed metric collector to crash the entire command display.

## Performance expectations

Target:

- Interactive startup in under 250 ms on a typical modern machine
- A default refresh interval between 500 ms and 2 seconds
- Low idle CPU consumption
- Bounded memory use during long-running sessions
- Minimal flicker and terminal output

These are targets, not permission to fake measurements. Include a simple way to measure startup time, CPU consumption, executable size, and memory consumption.

## Code-quality expectations

Structure the code so that:

- Metric collection is separate from presentation.
- OS-specific collectors are isolated behind a stable interface.
- Rendering can be tested without a live terminal.
- Sampling and rate calculations can be tested deterministically.
- The main loop does not become a monolithic function.
- Comments explain non-obvious decisions rather than restating code.
- Error messages are actionable.

Use real operating-system APIs or virtual filesystems such as `/proc` where appropriate. Account for the fact that macOS does not provide Linux's `/proc` interface.

## Required deliverables

Provide a complete buildable project containing:

1. Source code and build configuration
2. A concise README covering the language decision, architecture, controls, macOS and Linux implementation, release builds, and known limitations
3. Automated tests for sampling, rate calculations, layout, and terminal-independent rendering
4. Captures of the primary and secondary views at `120x36`
5. The exact build, test, lint, and performance-measurement commands used, with their results

Build and test the project before concluding. Report failures honestly. Do not substitute pseudocode, TODOs, placeholders, or omitted sections for implementation.

## Scope discipline

Implement the required experience completely before adding anything else.

Do not add configuration files, mouse support, plugin systems, extra themes, additional rendering protocols, process-management actions, or other unrequested features. Extra features receive no evaluation credit and must not delay or weaken the required interface, portability, testing, or verification.

## Final response format

Present your response in this order:

1. Engineering decision and architecture
2. Implemented project summary
3. Build, test, lint, and performance results
4. Primary and secondary view captures
5. Known limitations

Conclude with a brief captain's log describing what you built and verified. Record this in the project documentation as a Markdown file.

Computer, begin implementation. Maximum warp.
