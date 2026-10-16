//! What the build log looks like: a banner per numbered phase that says what is done and what
//! is still to come, and byte-progress lines for transfers.
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU64, Ordering::Relaxed},
};
use std::time::Instant;

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

static PLAN: OnceLock<&'static [&'static str]> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();

/// Call once from `job`; a bare `build_index SRC OUT` runs only the middle five phases.
pub fn plan_job() {
    let _ = PLAN.set(&JOB);
}

/// Starts phase `name`, one of the names above.
pub fn phase(name: &str) {
    let plan = *PLAN.get_or_init(|| &JOB[2..7]);
    let start = START.get_or_init(Instant::now);
    let Some(i) = plan.iter().position(|p| *p == name) else {
        println!("\n=== {name} ===");
        return;
    };
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
        start.elapsed().as_secs_f64()
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
}

/// Bytes moved so far out of `total`, reported every tenth of the way.
pub struct Progress {
    label: String,
    total: u64,
    done: AtomicU64,
    step: AtomicU64,
    skipped: AtomicU64,
    started: Instant,
}

impl Progress {
    pub fn new(label: String, total: u64) -> Arc<Self> {
        println!("  {label}: {} in total", gib(total));
        Arc::new(Self {
            label,
            total,
            done: AtomicU64::new(0),
            step: AtomicU64::new(0),
            skipped: AtomicU64::new(0),
            started: Instant::now(),
        })
    }

    /// Bytes that were already in place: counted as done, but not as speed.
    pub fn skip(&self, bytes: u64) {
        self.skipped.fetch_add(bytes, Relaxed);
        self.add(bytes);
    }

    pub fn add(&self, bytes: u64) {
        let done = self.done.fetch_add(bytes, Relaxed) + bytes;
        let pct = (done * 100 / self.total.max(1)).min(100);
        if self.step.fetch_max(pct / 10, Relaxed) < pct / 10 {
            let rate =
                done as f64 / (1 << 20) as f64 / self.started.elapsed().as_secs_f64().max(0.001);
            println!(
                "  {}: {pct}% ({} of {}, {rate:.0} MiB/s)",
                self.label,
                gib(done),
                gib(self.total)
            );
        }
    }
}

fn gib(bytes: u64) -> String {
    format!("{:.2} GiB", bytes as f64 / (1u64 << 30) as f64)
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
        phase("Upload");
    }
}
