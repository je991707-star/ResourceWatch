//! Bounded history series shared by the live monitor and the mock source.
//! Missing samples are stored as `None` (the chart breaks the line there).

use crate::snapshot::{DiskRate, History};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HistorySample {
    pub cpu: Option<f64>,
    pub gpu: Option<f64>,
    pub encode: Option<f64>,
    pub decode: Option<f64>,
    pub vram: Option<f64>,
    pub ram: Option<f64>,
    pub net_up: Option<f64>,
    pub net_down: Option<f64>,
    pub mem_pressure: Option<f64>,
    pub disk_read: Option<f64>,
    pub disk_write: Option<f64>,
}

/// Total read / write B/s over all disks; `None` when no disk has a rate.
pub fn disk_totals<'a>(
    rates: impl IntoIterator<Item = &'a DiskRate>,
) -> (Option<f64>, Option<f64>) {
    let sum = |acc: Option<f64>, v: Option<f64>| match (acc, v) {
        (a, None) => a,
        (a, Some(v)) => Some(a.unwrap_or(0.0) + v),
    };
    rates
        .into_iter()
        .fold((None, None), |(r, w), d| (sum(r, d.read), sum(w, d.write)))
}

#[derive(Debug, Default)]
pub struct HistoryBuffer {
    cpu: VecDeque<Option<f64>>,
    gpu: VecDeque<Option<f64>>,
    encode: VecDeque<Option<f64>>,
    decode: VecDeque<Option<f64>>,
    vram: VecDeque<Option<f64>>,
    ram: VecDeque<Option<f64>>,
    net_up: VecDeque<Option<f64>>,
    net_down: VecDeque<Option<f64>>,
    mem_pressure: VecDeque<Option<f64>>,
    disk_read: VecDeque<Option<f64>>,
    disk_write: VecDeque<Option<f64>>,
}

fn push(buf: &mut VecDeque<Option<f64>>, value: Option<f64>, limit: usize) {
    buf.push_back(value.filter(|v| v.is_finite()));
    while buf.len() > limit {
        buf.pop_front();
    }
}

fn vec(buf: &VecDeque<Option<f64>>) -> Vec<Option<f64>> {
    buf.iter().copied().collect()
}

impl HistoryBuffer {
    /// Appends one sample; each series keeps at most `limit` (≥ 2) points,
    /// dropping the oldest when `historyPoints` shrinks.
    pub fn push(&mut self, s: HistorySample, limit: u32) {
        let n = limit.max(2) as usize;
        push(&mut self.cpu, s.cpu, n);
        push(&mut self.gpu, s.gpu, n);
        push(&mut self.encode, s.encode, n);
        push(&mut self.decode, s.decode, n);
        push(&mut self.vram, s.vram, n);
        push(&mut self.ram, s.ram, n);
        push(&mut self.net_up, s.net_up, n);
        push(&mut self.net_down, s.net_down, n);
        push(&mut self.mem_pressure, s.mem_pressure, n);
        push(&mut self.disk_read, s.disk_read, n);
        push(&mut self.disk_write, s.disk_write, n);
    }

    pub fn to_history(&self) -> History {
        History {
            cpu: vec(&self.cpu),
            gpu: vec(&self.gpu),
            encode: vec(&self.encode),
            decode: vec(&self.decode),
            vram: vec(&self.vram),
            ram: vec(&self.ram),
            net_up: vec(&self.net_up),
            net_down: vec(&self.net_down),
            mem_pressure: vec(&self.mem_pressure),
            disk_read: vec(&self.disk_read),
            disk_write: vec(&self.disk_write),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_and_keeps_missing() {
        let mut h = HistoryBuffer::default();
        for i in 0..15 {
            h.push(
                HistorySample {
                    cpu: (i % 3 != 0).then_some(i as f64),
                    gpu: Some(f64::NAN),
                    ..Default::default()
                },
                10,
            );
        }
        let out = h.to_history();
        assert_eq!(out.cpu.len(), 10);
        // Samples 5..=14 are kept; multiples of 3 were missing.
        assert_eq!(out.cpu[0], Some(5.0));
        assert_eq!(out.cpu[1], None);
        assert!(out.gpu.iter().all(Option::is_none), "NaN stored as missing");
        h.push(HistorySample::default(), 4);
        let d = |r: Option<f64>, w: Option<f64>| DiskRate { read: r, write: w };
        assert_eq!(
            disk_totals(&[d(Some(1.0), None), d(Some(2.0), None)]),
            (Some(3.0), None)
        );
        assert_eq!(disk_totals(&[]), (None, None));
        assert_eq!(h.to_history().ram.len(), 4);
    }
}
