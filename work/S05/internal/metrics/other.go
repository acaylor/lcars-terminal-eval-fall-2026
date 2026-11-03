//go:build !linux && !darwin

package metrics

import "time"

// NewSampler returns a zero sampler on unsupported platforms so the UI can
// render its unavailable states everywhere.
func NewSampler() Sampler { return nullSampler{} }

type nullSampler struct{}

func (nullSampler) Sample() Snapshot { return Snapshot{At: time.Now()} }
func (nullSampler) Close()           {}
