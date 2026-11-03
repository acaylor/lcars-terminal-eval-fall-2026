package main

import (
	"flag"
	"fmt"
	"os"
	"strings"
	"time"

	"lcars/internal/app"
	"lcars/internal/metrics"
	"lcars/internal/ui"
)

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, "lcars:", err)
		os.Exit(1)
	}
}
func run() error {
	interval := flag.Duration("interval", time.Second, "telemetry sample interval (250ms to 10s)")
	color := flag.String("color", "auto", "auto, truecolor, 256, 16, or mono")
	ascii := flag.Bool("ascii", false, "use only ASCII glyphs")
	snapshot := flag.Bool("snapshot", false, "print one plain frame and exit (supports redirected output)")
	size := flag.String("size", "120x36", "snapshot dimensions, columns x rows")
	view := flag.String("view", "command", "initial view: command or diagnostics")
	flag.Parse()
	if flag.NArg() != 0 {
		return fmt.Errorf("unexpected argument %q; run lcars -help", flag.Arg(0))
	}
	if *interval < 250*time.Millisecond || *interval > 10*time.Second {
		return fmt.Errorf("--interval must be between 250ms and 10s")
	}
	if *view != "command" && *view != "diagnostics" {
		return fmt.Errorf("--view must be command or diagnostics")
	}
	validColor := false
	for _, v := range []string{"auto", "truecolor", "256", "16", "mono"} {
		if *color == v {
			validColor = true
		}
	}
	if !validColor {
		return fmt.Errorf("--color must be auto, truecolor, 256, 16, or mono")
	}
	locale := os.Getenv("LC_ALL")
	if locale == "" {
		locale = os.Getenv("LC_CTYPE")
	}
	if locale == "" {
		locale = os.Getenv("LANG")
	}
	if locale == "C" || locale == "POSIX" {
		*ascii = true
	}
	if *snapshot {
		var w, h int
		var extra string
		n, _ := fmt.Sscanf(*size, "%dx%d%s", &w, &h, &extra)
		if n != 2 || w < 1 || h < 1 || w > 400 || h > 160 {
			return fmt.Errorf("--size must be 1x1 through 400x160, for example 120x36")
		}
		collector := metrics.NewCollector()
		var engine metrics.Engine
		engine.Sample(collector.Collect(*view == "diagnostics"))
		time.Sleep(200 * time.Millisecond)
		s := engine.Sample(collector.Collect(*view == "diagnostics"))
		_, err := fmt.Print(ui.Render(w, h, ui.State{Snapshot: s, Now: time.Now(), Diagnostics: *view == "diagnostics", ASCII: *ascii}).Plain())
		return err
	}
	term := os.Getenv("TERM")
	if term == "" || strings.EqualFold(term, "dumb") {
		return fmt.Errorf("TERM is unset or unsuitable; use a VT-compatible terminal or --snapshot")
	}
	_, noColor := os.LookupEnv("NO_COLOR")
	return app.Run(app.Options{Interval: *interval, Color: ui.DetectColor(*color, term, os.Getenv("COLORTERM"), noColor), ASCII: *ascii, Diagnostics: *view == "diagnostics"})
}
