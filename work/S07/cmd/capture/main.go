// capture records real telemetry through the same canvas used by the live UI.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"lcars/internal/metrics"
	"lcars/internal/ui"
)

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
func run() error {
	out := flag.String("out", "artifacts", "capture directory")
	flag.Parse()
	if err := os.MkdirAll(*out, 0755); err != nil {
		return err
	}
	collector := metrics.NewCollector()
	var engine metrics.Engine
	var snapshot metrics.Snapshot
	history := make([]metrics.Value, 0, 40)
	for i := 0; i < 40; i++ {
		snapshot = engine.Sample(collector.Collect(true))
		history = append(history, snapshot.Usage)
		if i < 39 {
			time.Sleep(100 * time.Millisecond)
		}
	}
	for _, v := range []struct {
		name               string
		w, h               int
		diagnostics, ascii bool
	}{{"primary-120x36", 120, 36, false, false}, {"secondary-120x36", 120, 36, true, false}, {"compact-80x24", 80, 24, false, false}, {"ascii-80x24", 80, 24, false, true}} {
		canvas := ui.Render(v.w, v.h, ui.State{Snapshot: snapshot, Now: time.Now(), Diagnostics: v.diagnostics, ASCII: v.ascii, History: history})
		if v.name == "primary-120x36" {
			blank := 0
			for _, cell := range canvas.Cells {
				if cell.Rune == ' ' && cell.BG == ui.Background {
					blank++
				}
			}
			metadata := map[string]any{"captured_at": snapshot.At.Format(time.RFC3339), "columns": v.w, "rows": v.h, "empty_background_percent": 100 * float64(blank) / float64(len(canvas.Cells)), "source": "40 live OS samples, approximately 100ms apart; same canvas as interactive application", "network_interfaces_excluding_loopback": len(snapshot.Network), "visible_process_count": snapshot.ProcessCount, "readable_process_count": len(snapshot.Processes)}
			data, err := json.MarshalIndent(metadata, "", "  ")
			if err != nil {
				return err
			}
			if err := os.WriteFile(filepath.Join(*out, "capture-metadata.json"), append(data, '\n'), 0644); err != nil {
				return err
			}
		}
		path := filepath.Join(*out, v.name)
		if err := os.WriteFile(path+".txt", []byte(canvas.Plain()), 0644); err != nil {
			return err
		}
		if err := os.WriteFile(path+".ansi", []byte("\x1b[?7l"+ui.Diff(nil, canvas, ui.TrueColor)+"\x1b[0m\x1b[?7h\n"), 0644); err != nil {
			return err
		}
		f, err := os.Create(path + ".svg")
		if err != nil {
			return err
		}
		err = canvas.SVG(f)
		closeErr := f.Close()
		if err != nil {
			return err
		}
		if closeErr != nil {
			return closeErr
		}
		fmt.Println(path + ".svg")
	}
	return nil
}
