package app

import (
	"context"
	"fmt"
	"io"
	"os"
	"os/signal"
	"syscall"
	"time"

	"lcars/internal/metrics"
	"lcars/internal/terminal"
	"lcars/internal/ui"
)

type Options struct {
	Interval    time.Duration
	Color       ui.ColorMode
	ASCII       bool
	Diagnostics bool
}

type controller struct {
	state    ui.State
	engine   metrics.Engine
	requests chan bool
	results  chan metrics.Raw
	baseline bool
}

func newController(ctx context.Context, collector metrics.Collector, options Options) *controller {
	c := &controller{state: ui.State{Now: time.Now(), ASCII: options.ASCII, Diagnostics: options.Diagnostics}, requests: make(chan bool, 1), results: make(chan metrics.Raw, 1)}
	go func() {
		for {
			select {
			case <-ctx.Done():
				return
			case details := <-c.requests:
				raw := collector.Collect(details)
				select {
				case c.results <- raw:
				case <-ctx.Done():
					return
				}
			}
		}
	}()
	return c
}
func (c *controller) request() {
	if c.state.Paused {
		return
	}
	select {
	case c.requests <- c.state.Diagnostics:
	default:
	}
}
func (c *controller) accept(raw metrics.Raw) {
	if c.state.Paused {
		return
	}
	if c.baseline {
		c.engine = metrics.Engine{}
		c.baseline = false
	}
	c.state.Snapshot = c.engine.Sample(raw)
	c.state.History = append(c.state.History, c.state.Snapshot.Usage)
	if len(c.state.History) > 120 {
		copy(c.state.History, c.state.History[len(c.state.History)-120:])
		c.state.History = c.state.History[:120]
	}
}
func (c *controller) key(key byte) (quit bool) {
	switch key {
	case 'q', 'Q', 27, 3:
		return true
	case '?':
		c.state.Help = !c.state.Help
	case ' ':
		c.state.Paused = !c.state.Paused
		if !c.state.Paused {
			c.baseline = true
			c.request()
		}
	case 'p', 'P', '\t':
		c.state.Diagnostics = !c.state.Diagnostics
		c.state.CorePage = 0
		c.request()
	case ']':
		c.state.CorePage++
	case '[':
		c.state.CorePage = max(0, c.state.CorePage-1)
	}
	return false
}

func Run(options Options) (err error) {
	// Install signal handling before changing input mode, closing the startup window.
	signals := make(chan os.Signal, 8)
	signal.Notify(signals, os.Interrupt, syscall.SIGTERM, syscall.SIGHUP, syscall.SIGQUIT, syscall.SIGWINCH)
	defer signal.Stop(signals)
	session, err := terminal.Open(os.Stdin, os.Stdout)
	if err != nil {
		return err
	}
	defer func() {
		if e := session.Close(); err == nil {
			err = e
		}
	}()
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	c := newController(ctx, metrics.NewCollector(), options)
	input := make(chan []byte, 8)
	go func() {
		defer close(input)
		for {
			var buf [64]byte
			n, e := os.Stdin.Read(buf[:])
			if e != nil {
				return
			}
			data := append([]byte(nil), buf[:n]...)
			select {
			case input <- data:
			case <-ctx.Done():
				return
			}
		}
	}()
	w, h, err := terminal.Size(os.Stdout)
	if err != nil {
		return err
	}
	var previous *ui.Canvas
	paint := func() error {
		canvas := ui.Render(w, h, c.state)
		_, err := io.WriteString(os.Stdout, ui.Diff(previous, canvas, options.Color))
		previous = canvas
		return err
	}
	if err = paint(); err != nil {
		return err
	}
	c.request()
	refresh := time.NewTicker(options.Interval)
	defer refresh.Stop()
	clock := time.NewTicker(250 * time.Millisecond)
	defer clock.Stop()
	var escapeTimer *time.Timer
	var escape <-chan time.Time
	defer func() {
		if escapeTimer != nil {
			escapeTimer.Stop()
		}
	}()
	sequence := false
	for {
		dirty := false
		select {
		case sig := <-signals:
			if sig != syscall.SIGWINCH {
				return nil
			}
			nw, nh, e := terminal.Size(os.Stdout)
			if e == nil {
				w, h = nw, nh
				previous = nil
				dirty = true
			}
		case data, ok := <-input:
			if !ok {
				return nil
			}
			for _, key := range data {
				if escape != nil {
					escapeTimer.Stop()
					escape = nil
					if key == '[' || key == 'O' {
						sequence = true
						continue
					}
					return nil
				}
				if sequence {
					if key >= 0x40 && key <= 0x7e {
						sequence = false
					}
					continue
				}
				if key == 27 {
					escapeTimer = time.NewTimer(40 * time.Millisecond)
					escape = escapeTimer.C
					continue
				}
				if c.key(key) {
					return nil
				}
				dirty = true
			}
		case <-escape:
			return nil
		case raw := <-c.results:
			c.accept(raw)
			dirty = !c.state.Paused
		case <-refresh.C:
			c.request()
		case now := <-clock.C:
			if !c.state.Paused {
				c.state.Now = now
				c.state.Tick++
				dirty = true
			}
		}
		if dirty {
			if err = paint(); err != nil {
				return fmt.Errorf("terminal write failed: %w", err)
			}
		}
	}
}
