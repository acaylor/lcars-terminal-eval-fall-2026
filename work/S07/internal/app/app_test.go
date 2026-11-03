package app

import (
	"lcars/internal/metrics"
	"testing"
	"time"
)

func TestPauseAndResumeRebaseline(t *testing.T) {
	c := &controller{requests: make(chan bool, 1)}
	r := metrics.Raw{At: time.Now(), CPU: map[string]metrics.CPUCounter{"cpu": {Total: 100, Idle: 50}}}
	c.accept(r)
	c.key(' ')
	r2 := metrics.Raw{At: r.At.Add(time.Second), CPU: map[string]metrics.CPUCounter{"cpu": {Total: 200, Idle: 50}}}
	c.accept(r2)
	if c.state.Snapshot.At != r.At {
		t.Fatal("paused data changed")
	}
	c.key(' ')
	c.accept(r2)
	if c.state.Snapshot.Usage.OK {
		t.Fatal("resume reused stale baseline")
	}
	for i := 0; i < 1000; i++ {
		r2.At = r2.At.Add(time.Second)
		c.accept(r2)
	}
	if len(c.state.History) > 120 || cap(c.state.History) > 256 {
		t.Fatal("history is unbounded")
	}
}
func TestControls(t *testing.T) {
	c := &controller{requests: make(chan bool, 1)}
	c.key('p')
	if !c.state.Diagnostics {
		t.Fatal("p")
	}
	c.key('\t')
	if c.state.Diagnostics {
		t.Fatal("tab")
	}
	c.key('?')
	if !c.state.Help {
		t.Fatal("help")
	}
	c.key(']')
	if c.state.CorePage != 1 {
		t.Fatal("core page")
	}
	for _, key := range []byte{'q', 27, 3} {
		if !c.key(key) {
			t.Fatal("quit", key)
		}
	}
}
