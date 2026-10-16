//! What the build log looks like: a banner per numbered phase that says what is done and what
//! is still to come, progress lines for transfers and passes over files, a heartbeat while a
//! phase is quiet, and a closing summary with the warnings the build collected.
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicU64, Ordering::Relaxed},
    mpsc,
};
use std::time::{Duration, Instant};

/// `build_index job`, start to finish.
const JOB: [&str; 9] = [
    "Download models",
    "Download sources",
    "Parse places and boundaries",
    "Parse addresses",
    "Merge streets, postcodes and house numbers",
    "Build search index",
    "Build reverse lookup",
    "Verify",
    "Upload",
];

/// A phase that prints nothing for this long gets a line saying it is still running.
const QUIET: Duration = Duration::from_secs(30);

static PLAN: OnceLock<&'static [&'static str]> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();
static LAST_OUTPUT: AtomicU64 = AtomicU64::new(0);
static WARNINGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// (phase, when it began, rss in bytes when it ended); the running phase is last, without rss.
static PHASES: Mutex<Vec<(&'static str, Instant, Option<u64>)>> = Mutex::new(Vec::new());
static HEARTBEAT: Mutex<Option<Heartbeat>> = Mutex::new(None);

fn now_ms() -> u64 {
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Marks that something was just printed, so the heartbeat stays quiet.
pub fn touch() {
    LAST_OUTPUT.store(now_ms(), Relaxed);
}

/// Call once from `job`; a bare `build_index SRC OUT` runs only the middle five phases.
pub fn plan_job() {
    let _ = PLAN.set(&JOB);
}

pub fn in_job() -> bool {
    PLAN.get().is_some_and(|p| p.len() == JOB.len())
}

fn plan() -> &'static [&'static str] {
    PLAN.get_or_init(|| &JOB[2..7])
}

struct Heartbeat(
    Option<mpsc::Sender<()>>,
    Option<std::thread::JoinHandle<()>>,
);

impl Heartbeat {
    fn start(name: &'static str) -> Self {
        let (tx, rx) = mpsc::channel::<()>();
        let begun = Instant::now();
        let thread = std::thread::spawn(move || {
            while let Err(mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(Duration::from_secs(5))
            {
                if now_ms().saturating_sub(LAST_OUTPUT.load(Relaxed)) >= QUIET.as_millis() as u64 {
                    let rss = crate::mem::rss()
                        .map_or(String::new(), |r| format!(", rss {:.1} GB", r as f64 / GB));
                    println!(
                        "  still in {name}: {:.0}s so far{rss}",
                        begun.elapsed().as_secs_f64()
                    );
                    touch();
                }
            }
        });
        Self(Some(tx), Some(thread))
    }
}

impl Drop for Heartbeat {
    fn drop(&mut self) {
        drop(self.0.take());
        if let Some(t) = self.1.take() {
            let _ = t.join();
        }
    }
}

const GB: f64 = (1u64 << 30) as f64;

/// Starts phase `name`, one of the names above, and ends the previous one.
pub fn phase(name: &str) {
    let plan = plan();
    let Some(i) = plan.iter().position(|p| *p == name) else {
        println!("\n=== {name} ===");
        return;
    };
    close_phase();
    PHASES.lock().unwrap().push((plan[i], Instant::now(), None));
    let list = |steps: &[&str], from: usize| {
        steps
            .iter()
            .enumerate()
            .map(|(j, s)| format!("{} {s}", from + j + 1))
            .collect::<Vec<_>>()
            .join(" | ")
    };
    println!(
        "\n\n=== [{}/{}] {name} (elapsed {:.0}s) ===",
        i + 1,
        plan.len(),
        now_ms() as f64 / 1000.0
    );
    println!(
        "    done:      {}",
        if i == 0 {
            "nothing yet".into()
        } else {
            list(&plan[..i], 0)
        }
    );
    let rest = &plan[i + 1..];
    println!(
        "    remaining: {}",
        if rest.is_empty() {
            "none, this is the last step".into()
        } else {
            list(rest, i + 1)
        }
    );
    touch();
    *HEARTBEAT.lock().unwrap() = Some(Heartbeat::start(plan[i]));
}

/// Stops the heartbeat and stamps the running phase's end with the process's memory.
fn close_phase() {
    drop(HEARTBEAT.lock().unwrap().take());
    if let Some(last) = PHASES.lock().unwrap().last_mut() {
        last.2 = crate::mem::rss();
    }
}

/// Prints a warning now and repeats it in the closing summary.
pub fn warn(text: String) {
    println!("  warning: {text}");
    touch();
    WARNINGS.lock().unwrap().push(text);
}

/// Per-phase times and memory, then every warning; ends the last phase.
pub fn summary() {
    close_phase();
    let phases = PHASES.lock().unwrap();
    println!("\n\n=== Summary ===");
    for (i, (name, begun, rss)) in phases.iter().enumerate() {
        let end = phases.get(i + 1).map_or_else(Instant::now, |p| p.1);
        println!(
            "  {:>2}. {name:<44} {:>7.0}s{}",
            i + 1,
            end.duration_since(*begun).as_secs_f64(),
            rss.map_or(String::new(), |r| format!("   rss {:.1} GB", r as f64 / GB))
        );
    }
    let warnings = WARNINGS.lock().unwrap();
    if warnings.is_empty() {
        println!("  no warnings");
    } else {
        println!("  {} warnings:", warnings.len());
        for w in warnings.iter() {
            println!("    - {w}");
        }
    }
}

/// Work done so far out of `total` (bytes, or items such as row groups), reported every
/// tenth of the way.
pub struct Progress {
    label: String,
    total: u64,
    unit: Option<&'static str>,
    done: AtomicU64,
    step: AtomicU64,
    skipped: AtomicU64,
    started: Instant,
}

impl Progress {
    pub fn new(label: String, total: u64) -> Arc<Self> {
        Self::make(label, total, None)
    }

    /// Counts `unit`s (say "row groups") instead of bytes; no speed is shown.
    pub fn items(label: String, total: u64, unit: &'static str) -> Arc<Self> {
        Self::make(label, total, Some(unit))
    }

    fn make(label: String, total: u64, unit: Option<&'static str>) -> Arc<Self> {
        let p = Arc::new(Self {
            label,
            total,
            unit,
            done: AtomicU64::new(0),
            step: AtomicU64::new(0),
            skipped: AtomicU64::new(0),
            started: Instant::now(),
        });
        println!("  {}: {} in total", p.label, p.show(total));
        touch();
        p
    }

    fn show(&self, n: u64) -> String {
        match self.unit {
            Some(unit) => format!("{n} {unit}"),
            None => format!("{:.2} GiB", n as f64 / (1u64 << 30) as f64),
        }
    }

    /// Bytes that were already in place: counted as done, but not as speed.
    pub fn skip(&self, bytes: u64) {
        self.skipped.fetch_add(bytes, Relaxed);
        self.add(bytes);
    }

    pub fn add(&self, n: u64) {
        let done = self.done.fetch_add(n, Relaxed) + n;
        let pct = (done * 100 / self.total.max(1)).min(100);
        if self.step.fetch_max(pct / 10, Relaxed) < pct / 10 {
            let speed = if self.unit.is_some() {
                String::new()
            } else {
                let moved = done.saturating_sub(self.skipped.load(Relaxed)) as f64;
                let secs = self.started.elapsed().as_secs_f64().max(0.001);
                format!(", {:.0} MiB/s", moved / (1 << 20) as f64 / secs)
            };
            println!(
                "  {}: {pct}% ({} of {}{speed})",
                self.label,
                self.show(done),
                self.show(self.total)
            );
            touch();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_counts_to_total() {
        let p = Progress::new("x".into(), 1000);
        for _ in 0..10 {
            p.add(100);
        }
        assert_eq!(p.done.load(Relaxed), 1000);
        assert_eq!(p.step.load(Relaxed), 10);
        let items = Progress::items("y".into(), 3, "files");
        (0..3).for_each(|_| items.add(1));
        assert_eq!(items.step.load(Relaxed), 10);
        assert_eq!(items.show(3), "3 files");
    }

    #[test]
    fn summary_lists_phases_and_warnings() {
        plan_job();
        phase("Verify");
        warn("something odd".into());
        phase("Upload");
        summary();
        let phases = PHASES.lock().unwrap();
        assert_eq!(
            phases.iter().map(|p| p.0).collect::<Vec<_>>(),
            ["Verify", "Upload"]
        );
        assert!(phases[0].2.is_some() || crate::mem::rss().is_none());
        assert!(
            WARNINGS
                .lock()
                .unwrap()
                .contains(&"something odd".to_string())
        );
        assert!(HEARTBEAT.lock().unwrap().is_none());
    }
}
