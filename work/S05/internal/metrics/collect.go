package metrics

import "time"

// Collect assembles Rates from the sampler's previous and current samples.
// Callers invoke it with consecutive snapshots; the package keeps no
// global state.
func CollectRates(prev, cur Snapshot) Rates {
	r := ComputeRates(prev, cur)
	if cur.procTicks != nil {
		r.Procs = ComputeProcessRates(prev.procTicks, cur.procTicks, cur.procInfoCache, r.Elapsed, 5)
		r.ProcsOK = len(r.Procs) > 0 || r.ProcCount == 0
		if r.Elapsed <= 0 {
			// First sample: no CPU% yet, but show the memory-heaviest
			// processes so the diagnostic view is never blank.
			r.Procs = ComputeProcessRates(cur.procTicks, cur.procTicks, cur.procInfoCache, time.Hour, 5)
			r.ProcsOK = len(r.Procs) > 0
		}
	}
	r.ProcCount = cur.procCount
	return r
}

// UptimeSecs exposes uptime as whole seconds (used by tests/darwin).
func (s Snapshot) UptimeSecs() float64 { return s.Uptime.Seconds() }
