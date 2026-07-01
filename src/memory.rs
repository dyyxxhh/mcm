//! Automatic JVM heap (`-Xmx`) allocation.
//!
//! Mirrors HMCL's `getAllocatedMemory` algorithm exactly, then adds two
//! refinements HMCL lacks:
//!
//! 1. **32-bit JVM detection** — a 32-bit JVM cannot address more than
//!    ~1.25 GB of heap; capping prevents `Invalid maximum heap size`
//!    crashes that HMCL/PCL guard against in UI but not in the formula.
//! 2. **Cross-platform physical-memory probe** without a `sysinfo` crate
//!    dependency (reads `/proc/meminfo` on Linux, `sysctl` on macOS,
//!    `wmic`/PowerShell on Windows).
//!
//! Formula (64-bit JVM), identical to HMCL:
//! ```text
//! available = physical - 512 MB            // reserve for OS + launcher
//! if available <= 0: return minimum (512 MB)
//! threshold = 8 GB
//! if available <= threshold:
//!     suggested = available * 0.8
//! else:
//!     suggested = threshold * 0.8 + (available - threshold) * 0.2
//! return clamp(suggested, minimum=512 MB, cap=16 GB)
//! ```

use std::path::Path;
use std::process::Command;

/// 512 MB — system reserve (matches HMCL).
const SYSTEM_RESERVE_BYTES: u64 = 512 * 1024 * 1024;
/// 8 GB — HMCL's threshold between the 80% and 20% bands.
const THRESHOLD_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// 16 GB — hard cap (matches HMCL).
const CAP_BYTES: u64 = 16 * 1024 * 1024 * 1024;
/// 512 MB — minimum heap when auto-allocation cannot compute better.
const MINIMUM_BYTES: u64 = 512 * 1024 * 1024;
/// 1.25 GB — 32-bit JVM address-space ceiling (process VA is ~2-3 GB;
/// leaving room for metaspace/threads/code cache).
const BIT32_MAX_BYTES: u64 = 1_342_177_280;

/// Detect total physical memory of the host, in bytes.
///
/// Returns `None` if detection fails (caller falls back to a safe default).
pub(crate) fn total_physical_memory() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        if let Some(bytes) = total_physical_memory_linux() {
            return Some(bytes);
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(bytes) = total_physical_memory_macos() {
            return Some(bytes);
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(bytes) = total_physical_memory_windows() {
            return Some(bytes);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn total_physical_memory_linux() -> Option<u64> {
    let content = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            let kb: u64 = rest.trim().split_whitespace().next()?.parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn total_physical_memory_macos() -> Option<u64> {
    let out = Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    s.trim().parse::<u64>().ok()
}

#[cfg(target_os = "windows")]
fn total_physical_memory_windows() -> Option<u64> {
    // Try wmic first (widely available on Win10+).
    if let Some(bytes) = wmic_or_powershell(
        &["ComputerSystem", "get", "TotalPhysicalMemory"],
        "Get-CimInstance Win32_ComputerSystem | Select-Object -ExpandProperty TotalPhysicalMemory",
    ) {
        return Some(bytes);
    }
    None
}

#[cfg(target_os = "windows")]
fn wmic_or_powershell(wmic_args: &[&str], ps_cmd: &str) -> Option<u64> {
    // wmic path
    let out = Command::new("wmic").args(wmic_args).output();
    if let Ok(o) = out {
        let s = String::from_utf8_lossy(&o.stdout);
        for line in s.lines() {
            let t = line.trim();
            if !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) {
                if let Ok(n) = t.parse::<u64>() {
                    return Some(n);
                }
            }
        }
    }
    // PowerShell fallback (wmic deprecated on newer Windows 11).
    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", ps_cmd])
        .output();
    if let Ok(o) = out {
        let s = String::from_utf8_lossy(&o.stdout);
        let t = s.trim();
        if let Ok(n) = t.parse::<u64>() {
            return Some(n);
        }
    }
    None
}

/// Compute the recommended `-Xmx` value in bytes.
///
/// `is_64bit_jvm` should reflect the *selected* JVM (not the host). A 32-bit
/// JVM on a 64-bit host still must be capped.
pub(crate) fn auto_allocate_max_heap(physical_bytes: u64, is_64bit_jvm: bool) -> u64 {
    // 32-bit JVM: address space caps usable heap well below 2 GB.
    // HMCL/PCL enforce this in UI; we enforce it in the formula so CLI
    // users get the same safety.
    if !is_64bit_jvm {
        let quarter = physical_bytes / 4;
        return std::cmp::min(quarter, BIT32_MAX_BYTES);
    }

    // 64-bit JVM: HMCL's exact algorithm.
    let available = physical_bytes.saturating_sub(SYSTEM_RESERVE_BYTES);
    if available == 0 {
        return MINIMUM_BYTES;
    }

    let suggested = if available <= THRESHOLD_BYTES {
        (available as f64 * 0.8) as u64
    } else {
        (THRESHOLD_BYTES as f64 * 0.8 + (available - THRESHOLD_BYTES) as f64 * 0.2) as u64
    };

    let bounded = std::cmp::max(suggested, MINIMUM_BYTES);
    std::cmp::min(bounded, CAP_BYTES)
}

/// Format a byte count as a JVM `-Xmx` argument value (e.g. `4096m`).
///
/// Uses megabytes to avoid float rounding (`4096m` not `4g`) — HMCL does
/// the same for determinism.
pub(crate) fn format_xmx(bytes: u64) -> String {
    let mb = bytes / (1024 * 1024);
    // Never go below 512m (MINIMUM_BYTES in MB).
    let mb = std::cmp::max(mb, 512);
    format!("-Xmx{mb}m")
}

/// Probe whether the JVM at `java_path` is 64-bit.
///
/// Runs `java -version` and inspects stderr (Java 8) / stdout for the
/// substring `64-Bit`. Returns `true` on probe failure — a 32-bit JVM on a
/// modern system is rare, and auto-allocation with a 64-bit assumption is
/// safer than refusing to launch when the probe fails.
pub(crate) fn is_jvm_64bit(java_path: &Path) -> bool {
    let out = Command::new(java_path).arg("-version").output();
    match out {
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            let stdout = String::from_utf8_lossy(&o.stdout);
            stderr.contains("64-Bit") || stdout.contains("64-Bit")
        }
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gb(n: u64) -> u64 {
        n * 1024 * 1024 * 1024
    }

    #[test]
    fn bit32_jvm_capped_at_1_25gb_regardless_of_host_ram() {
        // A 32-bit JVM on a 32 GB host still can't use more than ~1.25 GB.
        let heap = auto_allocate_max_heap(gb(32), false);
        assert_eq!(heap, BIT32_MAX_BYTES);
    }

    #[test]
    fn bit32_jvm_on_4gb_host_uses_quarter() {
        // 4 GB / 4 = 1 GB (below the 1.25 GB cap).
        let heap = auto_allocate_max_heap(gb(4), false);
        assert_eq!(heap, gb(1));
    }

    #[test]
    fn bit64_jvm_4gb_host_matches_hmcl() {
        // available = 4GB - 512MB = 3.5GB; <= 8GB threshold → 80% = 2.8GB
        let heap = auto_allocate_max_heap(gb(4), true);
        assert_eq!(heap, ((gb(4) - 512 * 1024 * 1024) as f64 * 0.8) as u64);
    }

    #[test]
    fn bit64_jvm_8gb_host_at_threshold() {
        // available = 8GB - 512MB = 7.5GB; <= 8GB threshold → 80% = 6GB
        let heap = auto_allocate_max_heap(gb(8), true);
        assert_eq!(heap, ((gb(8) - 512 * 1024 * 1024) as f64 * 0.8) as u64);
    }

    #[test]
    fn bit64_jvm_16gb_host_above_threshold() {
        // available = 16GB - 512MB = 15.5GB
        // suggested = 8GB*0.8 + (15.5GB - 8GB)*0.2 = 6.4GB + 1.5GB = 7.9GB
        let heap = auto_allocate_max_heap(gb(16), true);
        let avail = gb(16) - 512 * 1024 * 1024;
        let expected = (THRESHOLD_BYTES as f64 * 0.8
            + (avail - THRESHOLD_BYTES) as f64 * 0.2) as u64;
        assert_eq!(heap, expected);
        assert!(heap < CAP_BYTES);
    }

    #[test]
    fn bit64_jvm_32gb_host_matches_hmcl_formula() {
        // 32 GB host: available = 32GB - 512MB = 31.5GB
        // suggested = 8GB*0.8 + (31.5GB - 8GB)*0.2 = 6.4GB + 4.7GB = 11.1GB
        // (Below the 16 GB cap — HMCL conservatively reserves for the OS
        // even on large hosts; the cap only kicks in around 57 GB+.)
        let heap = auto_allocate_max_heap(gb(32), true);
        let avail = gb(32) - 512 * 1024 * 1024;
        let expected = (THRESHOLD_BYTES as f64 * 0.8
            + (avail - THRESHOLD_BYTES) as f64 * 0.2) as u64;
        assert_eq!(heap, expected);
        assert!(heap < CAP_BYTES);
    }

    #[test]
    fn bit64_jvm_64gb_host_capped_at_16gb() {
        // 64 GB host: suggested would exceed 16 GB, so cap applies.
        let heap = auto_allocate_max_heap(gb(64), true);
        assert_eq!(heap, CAP_BYTES);
    }

    #[test]
    fn tiny_host_returns_minimum() {
        // 256 MB host: available after reserve = 0 → minimum 512 MB.
        let heap = auto_allocate_max_heap(256 * 1024 * 1024, true);
        assert_eq!(heap, MINIMUM_BYTES);
    }

    #[test]
    fn format_xmx_uses_megabytes() {
        assert_eq!(format_xmx(gb(4)), "-Xmx4096m");
        assert_eq!(format_xmx(gb(8)), "-Xmx8192m");
        assert_eq!(format_xmx(512 * 1024 * 1024), "-Xmx512m");
    }

    #[test]
    fn format_xmx_floors_below_minimum() {
        // Below 512 MB floors to 512m.
        assert_eq!(format_xmx(100 * 1024 * 1024), "-Xmx512m");
    }
}
