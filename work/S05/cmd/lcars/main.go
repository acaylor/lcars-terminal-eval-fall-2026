// Command lcars is an LCARS-inspired terminal system monitor.
package main

import (
	"flag"
	"fmt"
	"os"
	"time"

	"lcars/internal/app"
	"lcars/internal/render"
)

func main() {
	var (
		modeStr  = flag.String("color", "auto", "colour mode: auto|true|256|mono|never")
		ascii    = flag.Bool("ascii", false, "ASCII-safe degraded rendering")
		interval = flag.Duration("interval", time.Second, "refresh interval (500ms..2s honoured, others clamped)")
		once     = flag.Bool("once", false, "render one frame to stdout and exit (for captures/tests)")
	)
	flag.Parse()

	mode := render.ModeAuto
	switch *modeStr {
	case "true", "24bit", "truecolor":
		mode = render.ModeTrue
	case "256":
		mode = render.Mode256
	case "mono", "never", "none":
		mode = render.ModeMono
	case "auto":
	default:
		fmt.Fprintf(os.Stderr, "lcars: unknown --color %q (want auto|true|256|mono)\n", *modeStr)
		os.Exit(2)
	}
	iv := *interval
	if iv < 200*time.Millisecond {
		iv = 200 * time.Millisecond
	}
	if iv > 5*time.Second {
		iv = 5 * time.Second
	}

	opt := app.Options{Mode: mode, Ascii: *ascii, Interval: iv, Once: *once}
	if !*once && (!app.IsTerminal(os.Stdout) || !app.IsTerminal(os.Stdin)) {
		fmt.Fprintln(os.Stderr, "lcars: not a terminal — run interactively, or use --once to print one frame")
		os.Exit(1)
	}
	if err := app.Run(opt); err != nil {
		fmt.Fprintln(os.Stderr, "lcars:", err)
		os.Exit(1)
	}
}
