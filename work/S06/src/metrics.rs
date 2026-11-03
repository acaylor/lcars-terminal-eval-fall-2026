use crate::model::{Disk, Memory, NetworkCounters, Process, RateTracker, Snapshot, Swap};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{Disks, Networks, ProcessesToUpdate, System};

/// Owns all OS-backed collectors. Presentation code only sees `Snapshot`, which
/// keeps platform failures and process lifetimes out of the rendering path.
pub struct Collector {
    system: System,
    disks: Option<Disks>,
    networks: Option<Networks>,
    rates: RateTracker,
    previous_sample: Instant,
}

impl Collector {
    pub fn new() -> Result<Self, String> {
        let mut system = System::new();
        system.refresh_all();
        let disks = Some(Disks::new_with_refreshed_list());
        let networks = Some(Networks::new_with_refreshed_list());
        Ok(Self {
            system,
            disks,
            networks,
            rates: RateTracker::default(),
            previous_sample: Instant::now(),
        })
    }

    pub fn sample(&mut self, include_processes: bool) -> Snapshot {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.previous_sample);
        self.previous_sample = now;

        self.system.refresh_memory();
        self.system.refresh_cpu_usage();
        if include_processes {
            self.system.refresh_processes(ProcessesToUpdate::All, true);
        }

        let network = self.sample_network(elapsed);
        let disk = self.sample_disk();
        let total_memory = self.system.total_memory();
        let memory = (total_memory > 0).then(|| Memory {
            used: self.system.used_memory(),
            available: self.system.available_memory(),
            total: total_memory,
        });
        let total_swap = self.system.total_swap();
        let swap = (total_swap > 0).then(|| Swap {
            used: self.system.used_swap(),
            total: total_swap,
        });
        let (time, date) = local_date_time();
        let load = {
            let load = System::load_average();
            let values = [load.one, load.five, load.fifteen];
            values
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0)
                .then_some(values)
        };
        let mut top_processes = Vec::new();
        let process_count = include_processes.then(|| {
            let processes = self.system.processes();
            let mut ordered: Vec<_> = processes.values().collect();
            ordered.sort_by(|a, b| {
                b.cpu_usage()
                    .total_cmp(&a.cpu_usage())
                    .then_with(|| a.pid().as_u32().cmp(&b.pid().as_u32()))
            });
            top_processes = ordered
                .into_iter()
                .take(5)
                .map(|process| Process {
                    pid: process.pid().as_u32(),
                    name: process.name().to_string_lossy().into_owned(),
                    cpu: finite_usage(process.cpu_usage()),
                    memory: process.memory(),
                })
                .collect();
            processes.len()
        });

        Snapshot {
            time,
            date,
            hostname: System::host_name().unwrap_or_else(|| "UNKNOWN HOST".into()),
            os: System::long_os_version()
                .or_else(System::name)
                .unwrap_or_else(|| "UNKNOWN OPERATING SYSTEM".into()),
            architecture: std::env::consts::ARCH.to_uppercase(),
            uptime: Some(Duration::from_secs(System::uptime())),
            cpu: Some(finite_usage(self.system.global_cpu_usage())),
            cores: self
                .system
                .cpus()
                .iter()
                .map(|cpu| finite_usage(cpu.cpu_usage()))
                .collect(),
            load,
            memory,
            swap,
            disk,
            network,
            process_count,
            top_processes,
        }
    }

    fn sample_disk(&mut self) -> Option<Disk> {
        let disks = self.disks.as_mut()?;
        disks.refresh(true);
        let root = disks
            .iter()
            .find(|disk| disk.mount_point() == std::path::Path::new("/"))?;
        let total = root.total_space();
        (total > 0).then(|| Disk {
            used: total.saturating_sub(root.available_space()),
            available: root.available_space(),
            total,
        })
    }

    fn sample_network(&mut self, elapsed: Duration) -> Option<crate::model::NetworkRates> {
        let networks = self.networks.as_mut()?;
        networks.refresh(true);
        let mut counters = NetworkCounters::default();
        for (name, interface) in networks.list() {
            if name == "lo" || name == "lo0" || name.starts_with("loopback") {
                continue;
            }
            counters.received = counters.received.saturating_add(interface.total_received());
            counters.transmitted = counters
                .transmitted
                .saturating_add(interface.total_transmitted());
        }
        self.rates.update(counters, elapsed)
    }
}

fn finite_usage(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn local_date_time() -> (String, String) {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as libc::time_t;
    let mut broken_down = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: `localtime_r` receives a valid timestamp and a valid writable
    // `tm` pointer. Unlike `localtime`, it does not use shared static storage.
    let result = unsafe { libc::localtime_r(&raw const seconds, broken_down.as_mut_ptr()) };
    if result.is_null() {
        return ("--:--:--".into(), "DATE UNAVAILABLE".into());
    }
    // SAFETY: a non-null return from `localtime_r` initialized the structure.
    let tm = unsafe { broken_down.assume_init() };
    const WEEKDAYS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
    let weekday = usize::try_from(tm.tm_wday)
        .ok()
        .and_then(|day| WEEKDAYS.get(day))
        .unwrap_or(&"---");
    (
        format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec),
        format!(
            "{:04}-{:02}-{:02} {weekday}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday
        ),
    )
}
