//! Memory available to this process: the container's cgroup limit minus its usage, capped by the
//! host's MemAvailable. Page cache is excluded from usage because the kernel reclaims it.

const MB: u64 = 1 << 20;
/// Tantivy rejects smaller per-thread arenas.
const MIN_PER_THREAD: u64 = 15 * MB;
/// Below this each thread flushes tiny segments; use fewer threads instead.
const USEFUL_PER_THREAD: u64 = 64 * MB;
/// Tantivy rejects per-thread arenas of 4 GiB or more.
const MAX_PER_THREAD: u64 = 4000 * MB;
/// The layout measured for query speed (about 64 segments for the world); more is untested.
const MAX_TOTAL: u64 = 4096 * MB;
/// Smaller arenas save little next to the build's other memory but flush thousands of tiny
/// segments that make the index slow to build and to query.
pub const MIN_TOTAL: u64 = 1024 * MB;

fn field(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|l| {
        let mut w = l.split_whitespace();
        (w.next()?.trim_end_matches(':') == key).then(|| w.next()?.parse().ok())?
    })
}

fn cgroup_v2(max: &str, current: &str, stat: &str) -> Option<u64> {
    let max: u64 = max.trim().parse().ok()?; // "max" means unlimited
    let used = current
        .trim()
        .parse::<u64>()
        .ok()?
        .saturating_sub(field(stat, "file").unwrap_or(0));
    Some(max.saturating_sub(used))
}

fn cgroup_v1(limit: &str, usage: &str, stat: &str) -> Option<u64> {
    let limit: u64 = limit.trim().parse().ok()?;
    if limit >= 1 << 60 {
        return None; // unlimited
    }
    let cache = field(stat, "total_cache")
        .or_else(|| field(stat, "cache"))
        .unwrap_or(0);
    Some(limit.saturating_sub(usage.trim().parse::<u64>().ok()?.saturating_sub(cache)))
}

fn meminfo(text: &str) -> Option<u64> {
    field(text, "MemAvailable").map(|kb| kb * 1024)
}

/// macOS: free, inactive, speculative and purgeable pages.
fn vm_stat(text: &str) -> Option<u64> {
    let page: u64 = text
        .split("page size of ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let pages = |k: &str| {
        text.lines().find(|l| l.starts_with(k)).and_then(|l| {
            l.rsplit(':')
                .next()?
                .trim()
                .trim_end_matches('.')
                .parse::<u64>()
                .ok()
        })
    };
    let n: u64 = [
        "Pages free",
        "Pages inactive",
        "Pages speculative",
        "Pages purgeable",
    ]
    .iter()
    .filter_map(|k| pages(k))
    .sum();
    Some(n * page)
}

pub fn available() -> Option<u64> {
    let read = |p: &str| std::fs::read_to_string(p).ok();
    if cfg!(target_os = "macos") {
        let out = std::process::Command::new("vm_stat").output().ok()?;
        return vm_stat(&String::from_utf8_lossy(&out.stdout));
    }
    let v2 = (|| {
        cgroup_v2(
            &read("/sys/fs/cgroup/memory.max")?,
            &read("/sys/fs/cgroup/memory.current")?,
            &read("/sys/fs/cgroup/memory.stat")?,
        )
    })();
    let v1 = (|| {
        let d = "/sys/fs/cgroup/memory";
        cgroup_v1(
            &read(&format!("{d}/memory.limit_in_bytes"))?,
            &read(&format!("{d}/memory.usage_in_bytes"))?,
            &read(&format!("{d}/memory.stat"))?,
        )
    })();
    let host = read("/proc/meminfo").and_then(|t| meminfo(&t));
    [v2.or(v1), host].into_iter().flatten().min()
}

/// Memory used by this process. On macOS, RSS keeps pages the allocator has released until the
/// OS reclaims them, so use the physical footprint instead.
pub fn rss() -> Option<u64> {
    if cfg!(target_os = "macos") {
        let out = std::process::Command::new("footprint")
            .args(["-p", &std::process::id().to_string()])
            .output()
            .ok()?;
        return footprint(&String::from_utf8_lossy(&out.stdout));
    }
    field(&std::fs::read_to_string("/proc/self/status").ok()?, "VmRSS").map(|kb| kb * 1024)
}

/// `footprint -p` output: "... Footprint: 12 GB (16384 bytes per page)".
fn footprint(text: &str) -> Option<u64> {
    let mut w = text.split("Footprint:").nth(1)?.split_whitespace();
    let v: f64 = w.next()?.parse().ok()?;
    let unit: u64 = match w.next()? {
        "KB" => 1 << 10,
        "MB" => 1 << 20,
        "GB" => 1 << 30,
        _ => 1,
    };
    Some((v * unit as f64) as u64)
}

/// Writer threads and total arena: half the headroom, between MIN_TOTAL and MAX_TOTAL, with
/// fewer threads when each would get less than USEFUL_PER_THREAD.
pub fn writer_budget(headroom: u64, max_threads: usize) -> (usize, usize) {
    let total = (headroom / 2).clamp(MIN_TOTAL, MAX_TOTAL);
    let threads = (max_threads as u64).min(total / USEFUL_PER_THREAD).max(1);
    let per = (total / threads).clamp(MIN_PER_THREAD, MAX_PER_THREAD);
    (threads as usize, (per * threads) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    const GB: u64 = 1 << 30;

    #[test]
    fn cgroups_exclude_page_cache() {
        assert_eq!(
            cgroup_v2(
                "8589934592\n",
                "4294967296\n",
                "anon 1073741824\nfile 3221225472\n"
            ),
            Some(7 * GB)
        );
        assert_eq!(cgroup_v2("max\n", "1\n", ""), None);
        assert_eq!(
            cgroup_v1(
                "8589934592",
                "2147483648",
                "cache 1073741824\ntotal_cache 1073741824\n"
            ),
            Some(7 * GB)
        );
        assert_eq!(cgroup_v1("9223372036854771712", "1", ""), None);
        assert_eq!(
            meminfo("MemTotal: 100 kB\nMemAvailable:    2048 kB\n"),
            Some(2 * MB)
        );
        let vm = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free:  10.\nPages active: 99.\nPages inactive: 5.\nPages speculative: 1.\nPages purgeable: 4.\n";
        assert_eq!(vm_stat(vm), Some(20 * 16384));
        let fp = "bash [1]: 64-bit    Footprint: 12 GB (16384 bytes per page)\n";
        assert_eq!(footprint(fp), Some(12 * GB));
    }

    #[test]
    fn writer_budget_scales_with_headroom() {
        assert_eq!(writer_budget(64 * GB, 16), (16, 4 * GB as usize));
        assert_eq!(writer_budget(2 * GB, 16), (16, GB as usize));
        assert_eq!(writer_budget(3 * GB, 16), (16, 1536 * MB as usize));
        assert_eq!(writer_budget(0, 16), (16, GB as usize));
        assert_eq!(writer_budget(0, 4), (4, GB as usize));
        assert_eq!(writer_budget(64 * GB, 1), (1, 4000 * MB as usize));
    }
}
