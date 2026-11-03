package main

import (
	"flag"
	"fmt"
	"os"
	"strings"
	"time"

	"starfleet/internal/app"
)

func main() { os.Exit(run()) }
func run() int {
	interval := flag.Duration("interval", time.Second, "telemetry sample interval (500ms to 10s)")
	color := flag.String("color", "auto", "auto, true, 256, basic, or mono")
	ascii := flag.Bool("ascii", false, "use only ASCII glyphs")
	snapshot := flag.Bool("snapshot", false, "print a single plain frame without taking over the terminal")
	diagnostic := flag.Bool("diagnostic", false, "show diagnostics in snapshot mode")
	capture := flag.String("capture", "", "export real primary and secondary samples to this directory (SVG, text, ANSI)")
	w := flag.Int("width", 120, "snapshot/capture columns (1 to 500)")
	h := flag.Int("height", 36, "snapshot/capture rows (1 to 200)")
	flag.Parse()
	if flag.NArg() > 0 {
		fmt.Fprintln(os.Stderr, "lcars: unexpected arguments; use --help")
		return 2
	}
	if *interval < 500*time.Millisecond || *interval > 10*time.Second {
		fmt.Fprintln(os.Stderr, "lcars: --interval must be between 500ms and 10s")
		return 2
	}
	if !strings.Contains("|auto|true|256|basic|mono|", "|"+*color+"|") {
		fmt.Fprintln(os.Stderr, "lcars: --color must be auto, true, 256, basic, or mono")
		return 2
	}
	if *w < 1 || *w > 500 || *h < 1 || *h > 200 {
		fmt.Fprintln(os.Stderr, "lcars: --width must be 1..500 and --height 1..200")
		return 2
	}
	if *capture != "" {
		if err := app.Capture(*capture, *w, *h, *ascii); err != nil {
			fmt.Fprintln(os.Stderr, "lcars:", err)
			return 1
		}
		return 0
	}
	if *snapshot {
		fmt.Print(app.Snapshot(*w, *h, *ascii, *diagnostic))
		return 0
	}
	if err := app.Run(app.Options{Interval: *interval, Color: *color, ASCII: *ascii}); err != nil {
		fmt.Fprintln(os.Stderr, "lcars:", err)
		return 1
	}
	return 0
}
