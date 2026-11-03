package app

import (
	"fmt"
	"io"
	"os"
	"os/signal"
	"syscall"
	"time"

	"starfleet/internal/metrics"
	"starfleet/internal/terminal"
	"starfleet/internal/ui"
)

type Options struct {
	Interval time.Duration
	Color    string
	ASCII    bool
}
type controller struct {
	state     ui.State
	sample    metrics.Snapshot
	sampler   metrics.Sampler
	collector metrics.Collector
}

func (c *controller) collect() {
	c.sample = c.sampler.Sample(c.collector.Collect(c.state.Diagnostic))
	c.state.Tick++
	c.state.History = append(c.state.History, c.sample.CPU)
	if len(c.state.History) > 160 {
		copy(c.state.History, c.state.History[len(c.state.History)-160:])
		c.state.History = c.state.History[:160]
	}
}
func (c *controller) key(k byte) (quit bool, collect bool) {
	switch k {
	case 'q', 27, 3:
		return true, false
	case '?':
		c.state.Help = !c.state.Help
	case ' ':
		c.state.Paused = !c.state.Paused
		return false, !c.state.Paused
	case 'p', '\t':
		c.state.Diagnostic = !c.state.Diagnostic
		return false, !c.state.Paused
	}
	return false, false
}
func Run(opts Options) error {
	// Install signal handling before entering raw mode, closing the startup gap.
	signals := make(chan os.Signal, 8)
	signal.Notify(signals, os.Interrupt, syscall.SIGTERM, syscall.SIGHUP, syscall.SIGWINCH, syscall.SIGQUIT)
	defer signal.Stop(signals)
	session, err := terminal.Open()
	if err != nil {
		return err
	}
	defer session.Close()
	c := controller{collector: metrics.NewCollector(), state: ui.State{ASCII: opts.ASCII}}
	c.collect()
	renderer := terminal.Renderer{Mode: terminal.DetectColor(opts.Color)}
	draw := func() error {
		w, h, e := terminal.Size()
		if e != nil {
			return e
		}
		if w == 0 || h == 0 {
			return nil
		}
		return renderer.Render(os.Stdout, ui.Draw(w, h, c.sample, c.state))
	}
	if err := draw(); err != nil {
		return err
	}
	keys := make(chan byte, 32)
	inputErr := make(chan error, 1)
	done := make(chan struct{})
	defer close(done)
	go func() {
		b := make([]byte, 32)
		for {
			n, e := os.Stdin.Read(b)
			if e != nil {
				select {
				case inputErr <- e:
				case <-done:
				}
				return
			}
			for _, k := range b[:n] {
				select {
				case keys <- k:
				case <-done:
					return
				}
			}
		}
	}()
	ticker := time.NewTicker(opts.Interval)
	defer ticker.Stop()
	for {
		select {
		case sig := <-signals:
			if sig != syscall.SIGWINCH {
				return nil
			}
		case <-ticker.C:
			if c.state.Paused {
				continue
			}
			c.collect()
		case k := <-keys:
			quit, collect := c.key(k)
			if quit {
				return nil
			}
			if collect {
				c.collect()
			}
		case e := <-inputErr:
			if e == io.EOF {
				return nil
			}
			return fmt.Errorf("reading terminal input: %w", e)
		}
		if err := draw(); err != nil {
			return err
		}
	}
}

// Capture takes real interval samples and exports both views from the same data.
func Capture(path string, w, h int, ascii bool) error {
	if err := os.MkdirAll(path, 0755); err != nil {
		return err
	}
	c := metrics.NewCollector()
	sampler := metrics.Sampler{}
	first := sampler.Sample(c.Collect(true))
	time.Sleep(time.Second)
	s := sampler.Sample(c.Collect(true))
	for _, diag := range []bool{false, true} {
		name := "primary"
		if diag {
			name = "secondary"
		}
		canvas := ui.Draw(w, h, s, ui.State{Diagnostic: diag, ASCII: ascii, Tick: 1, History: []metrics.Value{first.CPU, s.CPU}})
		if err := os.WriteFile(path+"/"+name+".txt", []byte(canvas.Plain()), 0644); err != nil {
			return err
		}
		f, e := os.Create(path + "/" + name + ".svg")
		if e != nil {
			return e
		}
		e = canvas.SVG(f)
		closeErr := f.Close()
		if e != nil {
			return e
		}
		if closeErr != nil {
			return closeErr
		}
		f, e = os.Create(path + "/" + name + ".ansi")
		if e != nil {
			return e
		}
		renderer := terminal.Renderer{Mode: terminal.TrueColor}
		e = renderer.Render(f, canvas)
		closeErr = f.Close()
		if e != nil {
			return e
		}
		if closeErr != nil {
			return closeErr
		}
	}
	return nil
}
func Snapshot(w, h int, ascii bool, diag bool) string {
	c := metrics.NewCollector()
	s := metrics.Sampler{}
	sample := s.Sample(c.Collect(diag))
	return ui.Draw(w, h, sample, ui.State{ASCII: ascii, Diagnostic: diag}).Plain()
}
