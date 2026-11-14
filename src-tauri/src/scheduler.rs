//! Unified monitoring scheduler.
//!
//! One dedicated thread runs every monitoring cycle (all collectors →
//! snapshot → history → event). Properties:
//!
//! - **Single runner, no overlap**: the tick closure runs synchronously on the
//!   scheduler thread, so at most one sampling cycle exists at any time and
//!   there is no task queue that could grow.
//! - **Monotonic, fixed rate, skip missed**: due times are `Instant`s
//!   (CLOCK_MONOTONIC) advanced by whole intervals. If a cycle overruns or the
//!   thread was stalled, the missed due times are counted and skipped — never
//!   replayed (like Tokio's `MissedTickBehavior::Skip`).
//! - **Dynamic interval**: [`Scheduler::set_interval`] wakes the same thread;
//!   the next cycle is one new interval from "now". No second thread or timer.
//! - **Resilient**: a panicking cycle is caught and counted; the next cycle
//!   still runs. Individual sensor failures are `None` values inside a cycle.
//! - **Stoppable**: [`Scheduler::stop`] (also on drop) wakes and joins the
//!   thread.
//!
//! Tokio is not used: the app has no other async work, and a blocking thread
//! with `Condvar::wait_timeout` (monotonic on Linux) is the simplest single
//! scheduling mechanism that satisfies the above.

use serde::Serialize;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const MIN_INTERVAL: Duration = Duration::from_millis(1);

/// Pure fixed-rate schedule (no I/O, no clock access) so it can be tested
/// with synthetic `Instant`s.
#[derive(Debug, Clone)]
pub struct Schedule {
    interval: Duration,
    next_due: Instant,
}

impl Schedule {
    /// First cycle is due immediately.
    pub fn new(interval: Duration, now: Instant) -> Self {
        Self {
            interval: interval.max(MIN_INTERVAL),
            next_due: now,
        }
    }

    #[cfg(test)]
    pub fn interval(&self) -> Duration {
        self.interval
    }

    pub fn next_due(&self) -> Instant {
        self.next_due
    }

    /// Remaining wait, or `None` when the cycle is due.
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        (now < self.next_due).then(|| self.next_due - now)
    }

    /// The new interval counts from `now` (like the legacy `_rescheduleAll`).
    pub fn set_interval(&mut self, interval: Duration, now: Instant) {
        self.interval = interval.max(MIN_INTERVAL);
        self.next_due = now + self.interval;
    }

    /// After the cycle due at `next_due` finished at `finished`: move to the
    /// first grid point strictly after `finished`. Returns how many due times
    /// were skipped because the cycle overran or the thread was stalled.
    pub fn advance(&mut self, finished: Instant) -> u64 {
        let next = self.next_due + self.interval;
        if next > finished {
            self.next_due = next;
            return 0;
        }
        let step = self.interval.as_nanos();
        let skipped = (finished - next).as_nanos() / step + 1;
        let jump = u64::try_from(step * skipped).unwrap_or(u64::MAX);
        self.next_due = next + Duration::from_nanos(jump);
        u64::try_from(skipped).unwrap_or(u64::MAX)
    }
}

/// Counters for diagnostics and performance tests (`get_scheduler_stats`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerStats {
    pub interval_ms: u64,
    /// Completed cycles.
    pub ticks: u64,
    /// Due times skipped (overrun / stall / resume), never replayed.
    pub skipped: u64,
    /// Cycles that panicked (caught; scheduler kept running).
    pub panics: u64,
    pub last_duration_us: u64,
    pub max_duration_us: u64,
    pub total_duration_us: u64,
    /// Start delay relative to the due time.
    pub last_lateness_us: u64,
    pub max_lateness_us: u64,
    pub total_lateness_us: u64,
}

fn micros(d: Duration) -> u64 {
    u64::try_from(d.as_micros()).unwrap_or(u64::MAX)
}

impl SchedulerStats {
    fn record(&mut self, lateness: Duration, duration: Duration, skipped: u64, panicked: bool) {
        let (l, d) = (micros(lateness), micros(duration));
        self.ticks += 1;
        self.skipped += skipped;
        self.panics += u64::from(panicked);
        self.last_duration_us = d;
        self.max_duration_us = self.max_duration_us.max(d);
        self.total_duration_us = self.total_duration_us.saturating_add(d);
        self.last_lateness_us = l;
        self.max_lateness_us = self.max_lateness_us.max(l);
        self.total_lateness_us = self.total_lateness_us.saturating_add(l);
    }
}

struct Control {
    interval: Duration,
    interval_changed: bool,
    stop: bool,
    stats: SchedulerStats,
}

struct Shared {
    control: Mutex<Control>,
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Control> {
        self.control.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub struct Scheduler {
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Scheduler {
    /// Starts the single scheduler thread. `tick(now)` performs one complete
    /// monitoring cycle; `now` is the cycle's shared timestamp.
    pub fn start<F>(interval: Duration, tick: F) -> std::io::Result<Self>
    where
        F: FnMut(Instant) + Send + 'static,
    {
        let shared = Arc::new(Shared {
            control: Mutex::new(Control {
                interval,
                interval_changed: false,
                stop: false,
                stats: SchedulerStats {
                    interval_ms: u64::try_from(interval.as_millis()).unwrap_or(u64::MAX),
                    ..Default::default()
                },
            }),
            wake: Condvar::new(),
        });
        let worker = Arc::clone(&shared);
        let handle = std::thread::Builder::new()
            .name("rw-scheduler".into())
            .spawn(move || run(&worker, tick))?;
        Ok(Self {
            shared,
            thread: Mutex::new(Some(handle)),
        })
    }

    /// Applies a new interval to the running thread (no new thread/timer).
    pub fn set_interval(&self, interval: Duration) {
        let mut c = self.shared.lock();
        if c.interval != interval {
            c.interval = interval;
            c.interval_changed = true;
            c.stats.interval_ms = u64::try_from(interval.as_millis()).unwrap_or(u64::MAX);
            drop(c);
            self.shared.wake.notify_all();
        }
    }

    pub fn stats(&self) -> SchedulerStats {
        self.shared.lock().stats
    }

    /// Stops and joins the thread. Idempotent. Must not be called from the
    /// tick closure itself.
    pub fn stop(&self) {
        self.shared.lock().stop = true;
        self.shared.wake.notify_all();
        let handle = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(h) = handle {
            if h.join().is_err() {
                eprintln!("ResourceWatch: scheduler thread ended with a panic");
            }
        }
    }

    #[cfg(test)]
    pub fn is_running(&self) -> bool {
        self.thread
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|h| !h.is_finished())
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run<F: FnMut(Instant)>(shared: &Shared, mut tick: F) {
    let mut schedule = Schedule::new(shared.lock().interval, Instant::now());
    loop {
        let due = {
            let mut c = shared.lock();
            loop {
                if c.stop {
                    return;
                }
                let now = Instant::now();
                if c.interval_changed {
                    c.interval_changed = false;
                    schedule.set_interval(c.interval, now);
                }
                match schedule.wait(now) {
                    None => break,
                    Some(d) => {
                        c = match shared.wake.wait_timeout(c, d) {
                            Ok((g, _)) => g,
                            Err(e) => e.into_inner().0,
                        };
                    }
                }
            }
            schedule.next_due()
        };
        let started = Instant::now();
        let panicked = catch_unwind(AssertUnwindSafe(|| tick(started))).is_err();
        if panicked {
            eprintln!("ResourceWatch: a monitoring cycle panicked; continuing");
        }
        let finished = Instant::now();
        let skipped = schedule.advance(finished);
        shared.lock().stats.record(
            started.saturating_duration_since(due),
            finished - started,
            skipped,
            panicked,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    /// Simulates a scheduler driven by a synthetic clock: each cycle starts at
    /// its due time and takes `cost`. Returns (cycle start offsets, skipped).
    fn simulate(interval: u64, cost: u64, until: u64) -> (Vec<u64>, u64) {
        let t0 = Instant::now();
        let mut s = Schedule::new(ms(interval), t0);
        let (mut starts, mut skipped) = (Vec::new(), 0);
        loop {
            let start = s.next_due();
            if start > t0 + ms(until) {
                break;
            }
            starts.push((start - t0).as_millis() as u64);
            skipped += s.advance(start + ms(cost));
        }
        (starts, skipped)
    }

    #[test]
    fn fixed_rate_at_100_500_10000_ms() {
        for interval in [100, 500, 10_000] {
            let (starts, skipped) = simulate(interval, 3, interval * 20);
            assert_eq!(starts.len(), 21, "interval {interval}");
            assert!(
                starts.windows(2).all(|w| w[1] - w[0] == interval),
                "no drift"
            );
            assert_eq!(skipped, 0);
        }
    }

    #[test]
    fn overrun_skips_instead_of_queueing() {
        // Each cycle takes 250 ms with a 100 ms interval: cycles run back on
        // the 100 ms grid at most every 300 ms, two due times skipped each.
        let (starts, skipped) = simulate(100, 250, 3000);
        assert!(starts.windows(2).all(|w| w[1] - w[0] == 300), "{starts:?}");
        assert_eq!(skipped, 2 * starts.len() as u64);
    }

    #[test]
    fn cost_equal_to_interval_skips_one() {
        let t0 = Instant::now();
        let mut s = Schedule::new(ms(100), t0);
        // finished exactly on the next grid point: that point is not "after"
        assert_eq!(s.advance(t0 + ms(100)), 1);
        assert_eq!(s.next_due(), t0 + ms(200));
    }

    #[test]
    fn large_time_jump_runs_once_and_skips_rest() {
        // e.g. a stalled thread or clock jump of one hour at 500 ms.
        let t0 = Instant::now();
        let mut s = Schedule::new(ms(500), t0);
        assert_eq!(s.advance(t0 + ms(1)), 0);
        let resumed = t0 + Duration::from_secs(3600);
        assert_eq!(s.wait(resumed), None, "one cycle is due after resume");
        let skipped = s.advance(resumed + ms(2));
        assert_eq!(skipped, 7199);
        assert!(s.next_due() > resumed + ms(2));
        assert!(s.wait(resumed + ms(3)).is_some(), "no backlog left");
    }

    #[test]
    fn interval_change_counts_from_now() {
        let t0 = Instant::now();
        let mut s = Schedule::new(ms(10_000), t0);
        s.advance(t0 + ms(5));
        s.set_interval(ms(100), t0 + ms(20));
        assert_eq!(s.next_due(), t0 + ms(120));
        s.set_interval(ms(10_000), t0 + ms(50));
        assert_eq!(s.next_due(), t0 + ms(10_050));
        assert_eq!(s.interval(), ms(10_000));
        s.set_interval(Duration::ZERO, t0);
        assert_eq!(s.interval(), MIN_INTERVAL, "zero interval clamped");
    }

    #[test]
    fn thread_never_overlaps_and_skips_when_slow() {
        let busy = Arc::new(AtomicBool::new(false));
        let overlap = Arc::new(AtomicBool::new(false));
        let count = Arc::new(AtomicU64::new(0));
        let (b, o, c) = (busy.clone(), overlap.clone(), count.clone());
        let sched = Scheduler::start(ms(5), move |_| {
            if b.swap(true, Ordering::SeqCst) {
                o.store(true, Ordering::SeqCst);
            }
            std::thread::sleep(ms(12)); // slower than the interval
            c.fetch_add(1, Ordering::SeqCst);
            b.store(false, Ordering::SeqCst);
        })
        .unwrap();
        std::thread::sleep(ms(150));
        sched.stop();
        let stats = sched.stats();
        assert!(!overlap.load(Ordering::SeqCst));
        assert_eq!(stats.ticks, count.load(Ordering::SeqCst));
        assert!(stats.ticks >= 3 && stats.ticks <= 15, "{stats:?}");
        assert!(
            stats.skipped >= stats.ticks - 1,
            "overrun cycles skip due times: {stats:?}"
        );
        assert!(stats.max_duration_us >= 12_000);
    }

    #[test]
    fn single_thread_across_interval_changes_and_clean_stop() {
        let ids = Arc::new(Mutex::new(std::collections::HashSet::new()));
        let seen = ids.clone();
        let sched = Scheduler::start(ms(1000), move |_| {
            seen.lock().unwrap().insert(std::thread::current().id());
        })
        .unwrap();
        for i in 0..50 {
            sched.set_interval(ms(2 + i % 5));
        }
        std::thread::sleep(ms(60));
        assert!(sched.is_running());
        assert!(
            sched.stats().ticks > 3,
            "new interval applied without restart"
        );
        assert_eq!(sched.stats().interval_ms, 6);
        sched.stop();
        assert!(!sched.is_running(), "thread joined");
        let after = sched.stats().ticks;
        std::thread::sleep(ms(20));
        assert_eq!(sched.stats().ticks, after, "no ticks after stop");
        sched.stop(); // idempotent
        assert_eq!(ids.lock().unwrap().len(), 1, "exactly one scheduler thread");
    }

    #[test]
    fn panicking_cycle_does_not_stop_scheduler() {
        let n = Arc::new(AtomicU64::new(0));
        let n2 = n.clone();
        let sched = Scheduler::start(ms(3), move |_| {
            if n2.fetch_add(1, Ordering::SeqCst) == 1 {
                panic!("sensor exploded");
            }
        })
        .unwrap();
        std::thread::sleep(ms(40));
        sched.stop();
        let s = sched.stats();
        assert_eq!(s.panics, 1);
        assert!(s.ticks >= 3, "kept running after the panic: {s:?}");
    }

    #[test]
    fn long_interval_stop_is_prompt() {
        let sched = Scheduler::start(Duration::from_secs(10), |_| {}).unwrap();
        std::thread::sleep(ms(20));
        let t = Instant::now();
        sched.stop();
        assert!(t.elapsed() < ms(500), "stop wakes a sleeping 10 s wait");
        assert_eq!(sched.stats().ticks, 1, "only the immediate first cycle");
    }
}
