//! Selects where snapshots come from: the live [`Monitor`] (default) or the
//! [`MockSource`] when `RESOURCEWATCH_MOCK=1` is set (UI development only).

use crate::collectors::process::ProcessInfo;
use crate::mock::MockSource;
use crate::monitor::Monitor;
use crate::settings::Settings;
use crate::snapshot::Snapshot;
use serde::Deserialize;
use std::time::Instant;

/// Manual refresh actions (`refresh_devices` command).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RefreshKind {
    /// IPv4 address of the selected interface.
    Ipv4,
    /// Physical disk list.
    Disks,
    /// CPU model, RAM speed, GPU list and name.
    Hardware,
}

pub enum Source {
    Live(Box<Monitor>),
    Mock(Box<MockSource>),
}

impl Source {
    pub fn from_env(settings: &Settings) -> Self {
        if std::env::var("RESOURCEWATCH_MOCK").is_ok_and(|v| v == "1") {
            eprintln!("ResourceWatch: RESOURCEWATCH_MOCK=1, showing mock data");
            Source::Mock(Box::default())
        } else {
            Source::Live(Box::new(Monitor::new(settings)))
        }
    }

    /// One monitoring cycle at the scheduler's timestamp `now`.
    pub fn sample(&mut self, now: Instant, s: &Settings) -> Snapshot {
        match self {
            Source::Live(m) => m.sample(now, s),
            Source::Mock(m) => m.sample(s),
        }
    }

    pub fn peek(&mut self, s: &Settings) -> Snapshot {
        match self {
            Source::Live(m) => m.peek(s),
            Source::Mock(m) => m.peek(s),
        }
    }

    pub fn refresh(&mut self, kind: RefreshKind, s: &Settings) {
        match self {
            Source::Live(m) => m.refresh(kind, s),
            Source::Mock(m) => m.refresh(kind),
        }
    }

    /// Process list (`get_processes`), independent of the scheduler cycle.
    pub fn processes(&mut self, now: Instant) -> Vec<ProcessInfo> {
        match self {
            Source::Live(m) => m.processes(now),
            Source::Mock(m) => m.processes(),
        }
    }

    pub fn apply_settings(&mut self, s: &Settings) {
        if let Source::Live(m) = self {
            m.apply_settings(s);
        }
    }
}
