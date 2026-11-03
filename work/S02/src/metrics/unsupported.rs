//! Fallback for Unix systems without a dedicated collector: only the
//! portable metrics are reported; everything else shows as unavailable.

use super::common;
use super::*;

pub struct UnsupportedCollector;

impl Collector for UnsupportedCollector {
    fn system_info(&mut self) -> SystemInfo {
        let (sys, rel, machine) = common::uname();
        SystemInfo { os: sys.clone(), kernel: format!("{sys} {rel}"), arch: machine }
    }

    fn sample(&mut self, want_procs: bool) -> RawSample {
        let mut s = RawSample::empty(Missing::Unsupported);
        s.hostname = common::hostname();
        s.load = common::load();
        s.disk = common::disk("/", "/");
        s.procs = want_procs.then_some(Err(Missing::Unsupported));
        s
    }
}
