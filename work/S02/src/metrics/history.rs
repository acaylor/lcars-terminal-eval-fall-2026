//! Fixed-capacity ring buffers for traces. Memory stays bounded no matter
//! how long the monitor runs.

#[derive(Clone, Debug)]
pub struct Ring {
    buf: Vec<f32>,
    head: usize,
    len: usize,
}

impl Ring {
    pub fn new(cap: usize) -> Ring {
        Ring { buf: vec![0.0; cap.max(1)], head: 0, len: 0 }
    }
    pub fn push(&mut self, v: f32) {
        let cap = self.buf.len();
        self.buf[self.head] = v;
        self.head = (self.head + 1) % cap;
        self.len = (self.len + 1).min(cap);
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }
    /// The newest `n` values, oldest first.
    pub fn last_n(&self, n: usize) -> impl Iterator<Item = f32> + '_ {
        let n = n.min(self.len);
        let cap = self.buf.len();
        let start = (self.head + cap - n) % cap;
        (0..n).map(move |i| self.buf[(start + i) % cap])
    }
    pub fn max_last(&self, n: usize) -> f32 {
        self.last_n(n).fold(0.0, f32::max)
    }
}

pub const HISTORY_CAP: usize = 512;

/// Traces for the command view. Values: percentages 0..100 for cpu/mem,
/// bytes per second for network, raw load average for load.
#[derive(Clone, Debug)]
pub struct History {
    pub cpu: Ring,
    pub mem: Ring,
    pub rx: Ring,
    pub tx: Ring,
    pub load: Ring,
}

impl Default for History {
    fn default() -> Self {
        History {
            cpu: Ring::new(HISTORY_CAP),
            mem: Ring::new(HISTORY_CAP),
            rx: Ring::new(HISTORY_CAP),
            tx: Ring::new(HISTORY_CAP),
            load: Ring::new(HISTORY_CAP),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_keeps_newest_values_in_order() {
        let mut r = Ring::new(3);
        for v in 1..=5 {
            r.push(v as f32);
        }
        assert_eq!(r.len(), 3);
        assert_eq!(r.last_n(10).collect::<Vec<_>>(), vec![3.0, 4.0, 5.0]);
        assert_eq!(r.last_n(2).collect::<Vec<_>>(), vec![4.0, 5.0]);
        assert_eq!(r.max_last(3), 5.0);
    }

    #[test]
    fn ring_memory_is_bounded() {
        let mut r = Ring::new(HISTORY_CAP);
        for i in 0..100_000 {
            r.push(i as f32);
        }
        assert_eq!(r.len(), HISTORY_CAP);
        assert_eq!(r.capacity(), HISTORY_CAP);
    }
}
