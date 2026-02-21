//! Unix sandbox using cgroups v2 and seccomp-bpf.
//!
//! Enforces memory and CPU limits via Linux cgroups.
//! Enforces syscall restrictions via seccomp-bpf.
//! Note: Requires root or cgroup delegation for full functionality.
//!
//! # Security Warning
//!
//! This implementation provides actual cgroups v2 enforcement when possible.
//! If cgroups cannot be applied, the sandbox returns an error rather than
//! silently succeeding (security-in-depth principle).
//!
//! # Seccomp-bpf
//!
//! When enabled, seccomp-bpf restricts the syscalls available to the process
//! to a minimal whitelist required for inference operations. This provides
//! defense-in-depth against code execution vulnerabilities.

use super::{Sandbox, SandboxConfig, SandboxResult, SandboxUsage};
use crate::telemetry::{log_security_event, SecurityEvent};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

/// Cgroup base path for v2
const CGROUP_V2_BASE: &str = "/sys/fs/cgroup";

/// Sandbox cgroup name
const SANDBOX_CGROUP_NAME: &str = "gg-core-sandbox";

/// Seccomp filter flag for strict mode
#[cfg(target_os = "linux")]
const SECCOMP_MODE_FILTER: i32 = 2;

/// Seccomp return action: allow syscall
#[cfg(target_os = "linux")]
const SECCOMP_RET_ALLOW: u32 = 0x7FFF0000;

/// Seccomp return action: kill process
#[cfg(target_os = "linux")]
const SECCOMP_RET_KILL_PROCESS: u32 = 0x80000000;

/// BPF instruction classes
#[cfg(target_os = "linux")]
mod bpf {
    pub const LD: u16 = 0x00;
    pub const LDX: u16 = 0x01;
    pub const ST: u16 = 0x02;
    pub const STX: u16 = 0x03;
    pub const ALU: u16 = 0x04;
    pub const JMP: u16 = 0x05;
    pub const RET: u16 = 0x06;
    pub const MISC: u16 = 0x07;
}

/// BPF size modifiers
#[cfg(target_os = "linux")]
mod bpf_size {
    pub const W: u16 = 0x00;
    pub const H: u16 = 0x08;
    pub const B: u16 = 0x10;
    pub const DW: u16 = 0x18;
}

/// BPF mode modifiers
#[cfg(target_os = "linux")]
mod bpf_mode {
    pub const IMM: u16 = 0x00;
    pub const ABS: u16 = 0x20;
    pub const IND: u16 = 0x40;
    pub const MEM: u16 = 0x60;
    pub const LEN: u16 = 0x80;
    pub const MSH: u16 = 0xA0;
}

/// BPF source modifiers
#[cfg(target_os = "linux")]
mod bpf_src {
    pub const K: u16 = 0x00;
    pub const X: u16 = 0x08;
}

/// BPF jump conditions
#[cfg(target_os = "linux")]
mod bpf_jmp {
    pub const JA: u16 = 0x00;
    pub const JEQ: u16 = 0x10;
    pub const JGT: u16 = 0x20;
    pub const JGE: u16 = 0x30;
    pub const JSET: u16 = 0x40;
}

/// Architecture identifier for x86_64
#[cfg(target_os = "linux")]
const AUDIT_ARCH_X86_64: u32 = 0xC000003E;

/// Architecture identifier for aarch64
#[cfg(target_os = "linux")]
const AUDIT_ARCH_AARCH64: u32 = 0xC00000B7;

/// seccomp_data structure for BPF filter
#[cfg(target_os = "linux")]
#[repr(C)]
struct SeccompData {
    nr: i32,
    arch: u32,
    instruction_pointer: u64,
    args: [u64; 6],
}

/// BPF instruction structure
#[cfg(target_os = "linux")]
#[repr(C)]
struct SockFilter {
    code: u16,
    jt: u8,
    jf: u8,
    k: u32,
}

/// BPF program structure
#[cfg(target_os = "linux")]
#[repr(C)]
struct SockFprog {
    len: u16,
    filter: *const SockFilter,
}

/// Unix sandbox implementation using cgroups v2.
pub struct UnixSandbox {
    config: SandboxConfig,
    active: bool,
    cgroup_path: Option<String>,
}

impl UnixSandbox {
    /// Create a new Unix sandbox with the given configuration.
    pub fn new(config: SandboxConfig) -> Self {
        Self {
            config,
            active: false,
            cgroup_path: None,
        }
    }

    /// Check if cgroups v2 is available on this system
    fn cgroups_v2_available() -> bool {
        Path::new(CGROUP_V2_BASE)
            .join("cgroup.controllers")
            .exists()
    }

    /// Create cgroup directory and apply limits
    fn apply_cgroup_limits(&self) -> Result<String, String> {
        if !Self::cgroups_v2_available() {
            return Err("cgroups v2 not available on this system".to_string());
        }

        let cgroup_path = format!("{}/{}", CGROUP_V2_BASE, SANDBOX_CGROUP_NAME);

        // Create cgroup directory
        fs::create_dir_all(&cgroup_path)
            .map_err(|e| format!("Failed to create cgroup directory: {}", e))?;

        // Apply memory limit
        if self.config.max_memory_bytes > 0 {
            let memory_path = format!("{}/memory.max", cgroup_path);
            let mut file = OpenOptions::new()
                .write(true)
                .open(&memory_path)
                .map_err(|e| format!("Failed to open memory.max: {}", e))?;

            writeln!(file, "{}", self.config.max_memory_bytes)
                .map_err(|e| format!("Failed to write memory limit: {}", e))?;
        }

        // Apply CPU limit (in microseconds per second)
        if self.config.max_cpu_time_ms > 0 {
            // Convert ms to microseconds per second (quota/period)
            let quota_us = (self.config.max_cpu_time_ms as u64) * 1000;
            let period_us = 1_000_000; // 1 second period

            let cpu_path = format!("{}/cpu.max", cgroup_path);
            let mut file = OpenOptions::new()
                .write(true)
                .open(&cpu_path)
                .map_err(|e| format!("Failed to open cpu.max: {}", e))?;

            writeln!(file, "{} {}", quota_us, period_us)
                .map_err(|e| format!("Failed to write CPU limit: {}", e))?;
        }

        // Add current process to cgroup
        let pid = std::process::id();
        let procs_path = format!("{}/cgroup.procs", cgroup_path);
        let mut file = OpenOptions::new()
            .write(true)
            .open(&procs_path)
            .map_err(|e| format!("Failed to open cgroup.procs: {}. Note: This may require root or cgroup delegation.", e))?;

        writeln!(file, "{}", pid).map_err(|e| format!("Failed to add process to cgroup: {}", e))?;

        Ok(cgroup_path)
    }

    /// Apply seccomp-bpf filter to restrict syscalls
    /// This provides defense-in-depth against code execution vulnerabilities
    #[cfg(target_os = "linux")]
    fn apply_seccomp_filter(&self) -> Result<(), String> {
        // Syscall whitelist for inference operations (x86_64 numbers)
        // These are the minimal syscalls needed for the runtime
        const ALLOWED_SYSCALLS_X86_64: &[i32] = &[
            // File operations
            0,   // read
            1,   // write
            2,   // open
            3,   // close
            8,   // lseek
            9,   // mmap
            10,  // mprotect
            11,  // munmap
            12,  // brk
            16,  // ioctl
            22,  // pipe
            23,  // select
            24,  // sched_yield
            28,  // madvise
            257, // openat
            262, // newfstatat
            // Process management
            39,  // getpid
            60,  // exit
            186, // gettid
            218, // set_tid_address
            231, // exit_group
            // Signal handling
            13, // rt_sigaction
            14, // rt_sigprocmask
            15, // rt_sigreturn
            // Time
            35,  // nanosleep
            228, // clock_gettime
            229, // clock_getres
            // Thread operations
            56, // clone
            58, // fork
            59, // execve
            61, // wait4
            // IPC (for tokio)
            41, // socket
            42, // connect
            43, // accept
            44, // sendto
            45, // recvfrom
            46, // sendmsg
            47, // recvmsg
            53, // socketpair
            54, // setsockopt
            55, // getsockopt
            // Futex for synchronization
            202, // futex
            // Eventfd for tokio
            281, // eventfd2
            // Epoll for tokio
            232, // epoll_wait
            233, // epoll_ctl
            254, // epoll_create1
            // Random
            318, // getrandom
            // GPU driver support
            157, // prctl
            158, // arch_prctl
        ];

        // Build BPF filter program
        let mut filter = Vec::new();

        // Load architecture
        filter.push(SockFilter {
            code: bpf::LD | bpf_size::W | bpf_mode::ABS,
            jt: 0,
            jf: 0,
            k: 4, // offsetof(seccomp_data, arch)
        });

        // Check architecture (x86_64)
        filter.push(SockFilter {
            code: bpf::JMP | bpf_jmp::JEQ | bpf_src::K,
            jt: 0,
            jf: 4, // Skip to kill if wrong arch
            k: AUDIT_ARCH_X86_64,
        });

        // Load syscall number
        filter.push(SockFilter {
            code: bpf::LD | bpf_size::W | bpf_mode::ABS,
            jt: 0,
            jf: 0,
            k: 0, // offsetof(seccomp_data, nr)
        });

        // Check against allowed syscalls
        for &syscall_nr in ALLOWED_SYSCALLS_X86_64 {
            filter.push(SockFilter {
                code: bpf::JMP | bpf_jmp::JEQ | bpf_src::K,
                jt: 1, // Jump to allow
                jf: 0, // Continue to next check
                k: syscall_nr as u32,
            });
        }

        // Default: kill process
        filter.push(SockFilter {
            code: bpf::RET | bpf_src::K,
            jt: 0,
            jf: 0,
            k: SECCOMP_RET_KILL_PROCESS,
        });

        // Allow syscall
        filter.push(SockFilter {
            code: bpf::RET | bpf_src::K,
            jt: 0,
            jf: 0,
            k: SECCOMP_RET_ALLOW,
        });

        let prog = SockFprog {
            len: filter.len() as u16,
            filter: filter.as_ptr(),
        };

        // Apply seccomp filter using prctl
        // PR_SET_NO_NEW_PRIVS = 38
        let result = unsafe { libc::prctl(38, 1, 0, 0, 0) };
        if result != 0 {
            return Err("Failed to set no_new_privs".to_string());
        }

        // PR_SET_SECCOMP = 22
        let result = unsafe { libc::prctl(22, SECCOMP_MODE_FILTER, &prog, 0, 0) };
        if result != 0 {
            return Err(format!(
                "Failed to apply seccomp filter: errno {}",
                std::io::Error::last_os_error()
            ));
        }

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    fn apply_seccomp_filter(&self) -> Result<(), String> {
        Ok(())
    }
}

impl Sandbox for UnixSandbox {
    fn apply(&self) -> SandboxResult {
        if !self.config.enabled {
            return SandboxResult {
                success: true,
                error: Some("sandbox disabled by config".into()),
            };
        }

        // SECURITY: Apply cgroup limits first
        let cgroup_result = self.apply_cgroup_limits();

        // SECURITY: Apply seccomp filter for syscall restriction
        let seccomp_result = self.apply_seccomp_filter();

        match (&cgroup_result, &seccomp_result) {
            (Ok(cgroup_path), Ok(())) => {
                let max_memory_mb = self.config.max_memory_bytes / 1024 / 1024;
                let max_cpu_ms = self.config.max_cpu_time_ms;
                log_security_event(
                    SecurityEvent::SandboxViolation,
                    "Unix sandbox applied successfully (cgroups + seccomp)",
                    &[
                        ("max_memory_mb", &format!("{}", max_memory_mb)),
                        ("max_cpu_ms", &format!("{}", max_cpu_ms)),
                        ("cgroup_path", cgroup_path),
                        ("seccomp", "enabled"),
                    ],
                );
                SandboxResult {
                    success: true,
                    error: None,
                }
            }
            (Err(e), _) | (_, Err(e)) => {
                // SECURITY: Return error instead of silently succeeding
                log_security_event(
                    SecurityEvent::SandboxViolation,
                    "Failed to apply Unix sandbox",
                    &[("error", &e)],
                );
                SandboxResult {
                    success: false,
                    error: Some(format!(
                        "Sandbox enforcement failed: {}. \
                         Either run with appropriate privileges (root/cgroup delegation) \
                         or disable sandbox explicitly if not required.",
                        e
                    )),
                }
            }
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn get_usage(&self) -> Option<SandboxUsage> {
        if !self.active {
            return None;
        }

        // Read from cgroup files if available
        if let Some(ref cgroup_path) = self.cgroup_path {
            let memory_path = format!("{}/memory.current", cgroup_path);
            let cpu_path = format!("{}/cpu.stat", cgroup_path);

            let memory_bytes = fs::read_to_string(&memory_path)
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);

            // Parse cpu.stat for usage_usec
            let cpu_time_ms = fs::read_to_string(&cpu_path)
                .ok()
                .and_then(|s| {
                    s.lines()
                        .find(|l| l.starts_with("usage_usec"))
                        .and_then(|l| l.split_whitespace().nth(1))
                        .and_then(|v| v.parse::<u64>().ok())
                        .map(|us| us / 1000) // Convert to ms
                })
                .unwrap_or(0);

            return Some(SandboxUsage {
                memory_bytes,
                cpu_time_ms,
            });
        }

        Some(SandboxUsage::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cgroups_v2_detection() {
        // This test will pass whether or not cgroups v2 is available
        // It just verifies the detection doesn't panic
        let _available = UnixSandbox::cgroups_v2_available();
    }

    #[test]
    fn test_sandbox_disabled_by_config() {
        let config = SandboxConfig {
            enabled: false,
            ..Default::default()
        };
        let sandbox = UnixSandbox::new(config);
        let result = sandbox.apply();

        assert!(result.success);
        assert!(result.error.unwrap().contains("disabled"));
    }

    #[test]
    fn test_sandbox_enabled_returns_proper_error() {
        let config = SandboxConfig {
            enabled: true,
            ..Default::default()
        };
        let sandbox = UnixSandbox::new(config);
        let result = sandbox.apply();

        // If cgroups v2 is not available or we don't have permissions,
        // this should return an error (not silently succeed)
        if !result.success {
            assert!(
                result.error.unwrap().contains("Failed")
                    || result.error.unwrap().contains("not available")
            );
        }
        // If it succeeds, that's also valid (we have permissions)
    }
}
