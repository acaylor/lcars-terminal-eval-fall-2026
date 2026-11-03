package app

import (
	"fmt"
	"os"
	"os/signal"
	"strings"
	"sync"
	"syscall"
	"time"

	"lcars/internal/metrics"
	"lcars/internal/render"

	"golang.org/x/term"
)

type Options struct {
	Mode     render.ColorMode
	Ascii    bool
	Interval time.Duration
	Once     bool
	NoAlt    bool
}

func millis(d time.Duration) int64 { return d.Milliseconds() }

// isTerminal reports whether f is a character device (tty).
func IsTerminal(f *os.File) bool {
	st, err := f.Stat()
	return err == nil && st.Mode()&os.ModeCharDevice != 0
}

// readSize delegates to render's size probe via terminal.GetSize fallback.
func readTermSize() (int, int) {
	if w, h, err := term.GetSize(int(os.Stdout.Fd())); err == nil && w > 0 {
		return w, h
	}
	if w, h, err := term.GetSize(int(os.Stdin.Fd())); err == nil && w > 0 {
		return w, h
	}
	if v := os.Getenv("COLUMNS"); v != "" {
		// best effort; http not needed
		_ = v
	}
	return 80, 24
}

// detectMode resolves the colour mode from flags/env without terminfo.
func DetectMode(optMode render.ColorMode) render.ColorMode {
	if optMode != render.ModeAuto {
		return optMode
	}
	if os.Getenv("NO_COLOR") != "" {
		return render.ModeMono
	}
	ct := strings.ToLower(os.Getenv("COLORTERM"))
	if ct == "truecolor" || ct == "24bit" {
		return render.ModeTrue
	}
	t := strings.ToLower(os.Getenv("TERM"))
	switch {
	case t == "dumb" || t == "":
		return render.ModeMono
	case strings.Contains(t, "256color"):
		return render.Mode256
	}
	return render.Mode256
}

type runner struct {
	sampler  metrics.Sampler
	prev     metrics.Snapshot
	havePrev bool
	state    State
	rd       *render.Renderer
	opt      Options
}

// Run is the entry point. It sets up the terminal (unless opts.Once),
// drives the tick loop, and restores state on every exit path.
func Run(opt Options) error {
	if opt.Interval <= 0 {
		opt.Interval = time.Second
	}
	if opt.Once {
		return runOnce(opt)
	}
	if !IsTerminal(os.Stdout) || !IsTerminal(os.Stdin) {
		return fmt.Errorf("lcars: stdout/stdin is not a terminal (use --once for a static frame)")
	}

	mode := DetectMode(opt.Mode)
	ascii := opt.Ascii || os.Getenv("LCARS_ASCII") != "" || !utf8OK()

	fd := int(os.Stdin.Fd())
	oldState, err := term.MakeRaw(fd)
	if err != nil {
		return fmt.Errorf("lcars: cannot set raw mode: %w", err)
	}
	// Restore on every path below.
	var once sync.Once
	restore := func() {
		once.Do(func() {
			fmt.Fprint(os.Stdout, "\x1b[?25h\x1b[?1049l\x1b[0m")
			_ = term.Restore(fd, oldState)
		})
	}
	defer restore()

	fmt.Fprint(os.Stdout, "\x1b[?1049h\x1b[?25l")
	rd := render.NewRenderer(os.Stdout, mode)

	r := &runner{sampler: metrics.NewSampler(), rd: rd, opt: opt}
	r.state = State{Mode: mode, Ascii: ascii, StartedAt: time.Now(), CPUScale: 0.1}
	defer r.sampler.Close()

	keys := make(chan []byte, 8)
	go readKeys(keys)

	sig := make(chan os.Signal, 4)
	signal.Notify(sig, syscall.SIGINT, syscall.SIGTERM, syscall.SIGHUP, syscall.SIGWINCH)
	defer signal.Stop(sig)

	tick := time.NewTicker(opt.Interval)
	defer tick.Stop()
	// Animation tick (~4 fps) for blinking/status motion while paused.
	anim := time.NewTicker(400 * time.Millisecond)
	defer anim.Stop()

	w, h := readTermSize()
	r.state.TermW, r.state.TermH = w, h
	r.sample()
	r.draw()

	for {
		select {
		case b := <-keys:
			if r.handleKeys(b) {
				return nil
			}
			r.draw()
		case s := <-sig:
			if s == syscall.SIGWINCH {
				nw, nh := readTermSize()
				if nw != r.state.TermW || nh != r.state.TermH {
					r.state.TermW, r.state.TermH = nw, nh
					r.rd.Invalidate()
				}
				r.draw()
				continue
			}
			return nil // SIGINT/SIGTERM/SIGHUP: defer restore()
		case <-tick.C:
			if !r.state.Paused {
				r.sample()
			}
			if !r.state.Help && r.state.View == ViewDiag {
				// keep diag fresh too
				if r.state.Paused {
					// frozen: still animate cursor-free blink
				}
			}
			r.draw()
		case <-anim.C:
			if r.state.Paused || r.state.Help {
				r.state.Tick++
				r.draw()
			}
		}
	}
}

func runOnce(opt Options) error {
	mode := DetectMode(opt.Mode)
	// --once: never enter alt screen; write one frame then restore cursor.
	ascii := opt.Ascii || os.Getenv("LCARS_ASCII") != ""
	r := &runner{sampler: metrics.NewSampler(), opt: opt}
	defer r.sampler.Close()
	r.state = State{Mode: mode, Ascii: ascii, StartedAt: time.Now(), CPUScale: 0.1}
	w, h := readTermSize()
	r.state.TermW, r.state.TermH = w, h
	r.sample()
	r.sample() // second sample so rates exist
	buf := Build(&r.state)
	var sb strings.Builder
	// Render one frame as plain ANSI rows (no alt screen).
	rd := render.NewRenderer(&sb, mode)
	rd.Render(buf)
	// Strip the CUP park sequence crudely: write as-is; acceptable for capture.
	os.Stdout.WriteString("\x1b[H\x1b[2J" + sb.String() + "\n")
	return nil
}

func (r *runner) sample() {
	snap := r.sampler.Sample()
	r.state.Snap = snap
	if r.havePrev {
		r.state.Rates = metrics.CollectRates(r.prev, snap)
		r.state.HasRates = true
	}
	r.prev = snap
	r.havePrev = true
	r.state.Tick++
	r.state.pushSample()
}

func (r *runner) draw() {
	buf := Build(&r.state)
	r.rd.Render(buf)
}

// handleKeys returns true when the app should quit.
func (r *runner) handleKeys(b []byte) bool {
	for len(b) > 0 {
		c := b[0]
		// Escape sequences: swallow CSI/SS3, treat lone Esc as quit.
		if c == 0x1b {
			if len(b) == 1 {
				return true
			}
			n := escLen(b)
			if n > 0 {
				b = b[n:]
				continue
			}
			return true
		}
		if c == 0x03 { // Ctrl-C byte (raw mode: no SIGINT from tty)
			return true
		}
		switch c {
		case 'q':
			return true
		case '?':
			r.state.Help = !r.state.Help
			r.rd.Invalidate()
		case ' ':
			r.state.Paused = !r.state.Paused
		case 'p':
			if r.state.View == ViewDiag {
				r.state.View = ViewPrimary
			} else {
				r.state.View = ViewDiag
			}
			r.rd.Invalidate()
		case '\t':
			if r.state.View == ViewPrimary {
				r.state.View = ViewDiag
			} else {
				r.state.View = ViewPrimary
			}
			r.rd.Invalidate()
		}
		b = b[1:]
	}
	return false
}

// escLen returns the length of an escape sequence at b[0]==ESC, or 0 if it
// looks like a bare Esc keypress (handled by caller after a timeout in a
// real impl; here the read is chunked so '[..' arrives together).
func escLen(b []byte) int {
	if len(b) >= 2 && (b[1] == '[' || b[1] == 'O') {
		for i := 2; i < len(b) && i < 16; i++ {
			if b[i] >= 0x40 && b[i] <= 0x7e {
				return i + 1
			}
		}
		return len(b)
	}
	return 0
}

func readKeys(ch chan<- []byte) {
	buf := make([]byte, 64)
	for {
		n, err := os.Stdin.Read(buf)
		if err != nil || n == 0 {
			return
		}
		b := make([]byte, n)
		copy(b, buf[:n])
		ch <- b
	}
}

func utf8OK() bool {
	for _, e := range []string{os.Getenv("LC_ALL"), os.Getenv("LC_CTYPE"), os.Getenv("LANG")} {
		if strings.Contains(strings.ToUpper(e), "UTF") {
			return true
		}
	}
	// Assume UTF-8 on modern terminals when locale is unset.
	return os.Getenv("LANG") == ""
}

var _ = millis
