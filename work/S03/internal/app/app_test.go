package app

import (
	"testing"
	"time"

	"starfleet/internal/metrics"
)

type fakeCollector struct{ calls int }

func (f *fakeCollector) Collect(bool) metrics.Raw {
	f.calls++
	return metrics.Raw{At: time.Unix(int64(f.calls), 0), CPUs: []metrics.CPU{{Busy: uint64(f.calls), Total: uint64(f.calls * 2)}}}
}
func TestControlsPauseAndBoundedHistory(t *testing.T) {
	f := &fakeCollector{}
	c := controller{collector: f}
	c.collect()
	quit, collect := c.key(' ')
	if quit || collect || !c.state.Paused {
		t.Fatal("pause")
	}
	_, collect = c.key('p')
	if collect || !c.state.Diagnostic {
		t.Fatal("view switch while paused")
	}
	_, collect = c.key(' ')
	if !collect || c.state.Paused {
		t.Fatal("resume")
	}
	c.key('?')
	if !c.state.Help {
		t.Fatal("help")
	}
	c.key('\t')
	if c.state.Diagnostic {
		t.Fatal("Tab")
	}
	for _, k := range []byte{'q', 27, 3} {
		if quit, _ := c.key(k); !quit {
			t.Fatal("quit")
		}
	}
	for i := 0; i < 500; i++ {
		c.collect()
	}
	if len(c.state.History) != 160 {
		t.Fatal("unbounded history")
	}
}
