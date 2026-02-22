//! Windows sandbox using Job Objects.
//!
//! Enforces memory and CPU limits via Windows Job Objects API.
//! SECURITY: This module provides OS-level resource isolation.

use super::{Sandbox, SandboxConfig, SandboxResult, SandboxUsage};
use crate::telemetry::{log_security_event, SecurityEvent};

/// Windows sandbox implementation using Job Objects.
pub struct WindowsSandbox {
    config: SandboxConfig,
    active: bool,
    job_handle: Option<isize>, // HANDLE is isize on Windows
}

impl WindowsSandbox {
    /// Create a new Windows sandbox with the given configuration.
    pub fn new(config: SandboxConfig) -> Self {
        Self {
            config,
            active: false,
            job_handle: None,
        }
    }
}

impl Sandbox for WindowsSandbox {
    fn apply(&self) -> SandboxResult {
        if !self.config.enabled {
            return SandboxResult {
                success: true,
                error: Some("sandbox disabled by config".into()),
            };
        }

        #[cfg(target_os = "windows")]
        {
            match apply_job_object_limits(&self.config) {
                Ok(_handle) => {
                    let max_memory_mb = self.config.max_memory_bytes / 1024 / 1024;
                    let max_cpu_ms = self.config.max_cpu_time_ms;
                    log_security_event(
                        SecurityEvent::SandboxViolation,
                        "Windows Job Object sandbox applied successfully",
                        &[
                            ("max_memory_mb", &format!("{}", max_memory_mb)),
                            ("max_cpu_ms", &format!("{}", max_cpu_ms)),
                        ],
                    );
                    SandboxResult {
                        success: true,
                        error: None,
                    }
                }
                Err(e) => {
                    log_security_event(
                        SecurityEvent::SandboxViolation,
                        "Failed to apply Windows Job Object sandbox",
                        &[("error", &e)],
                    );
                    SandboxResult {
                        success: false,
                        error: Some(e),
                    }
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            SandboxResult {
                success: true,
                error: Some("Windows sandbox called on non-Windows platform".into()),
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

        #[cfg(target_os = "windows")]
        {
            // QueryInformationJobObject would go here to get:
            // - TotalUserTime for CPU time
            // - PeakProcessMemoryUsed for memory
            Some(SandboxUsage::default())
        }

        #[cfg(not(target_os = "windows"))]
        {
            None
        }
    }
}

#[cfg(target_os = "windows")]
fn apply_job_object_limits(config: &SandboxConfig) -> Result<isize, String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_JOB_MEMORY,
        JOB_OBJECT_LIMIT_JOB_TIME,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    // SAFETY: All Windows API calls below operate on valid handles and zeroed structs.
    // Job object is created, configured, and assigned to the current process atomically.
    unsafe {
        // Create a job object
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job == 0 {
            return Err("Failed to create job object".to_string());
        }

        // Configure limits
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();

        // Set memory limit (if configured)
        if config.max_memory_bytes > 0 {
            info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_JOB_MEMORY;
            info.JobMemoryLimit = config.max_memory_bytes;
        }

        // Set CPU time limit (if configured)
        if config.max_cpu_time_ms > 0 {
            info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_JOB_TIME;
            // CPU time is in 100ns units
            info.BasicLimitInformation.PerJobUserTimeLimit =
                (config.max_cpu_time_ms as i64) * 10_000;
        }

        // Apply the limits
        let result = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );

        if result == 0 {
            CloseHandle(job);
            return Err("Failed to set job object limits".to_string());
        }

        // Assign the current process to the job object
        // This is required for the limits to actually apply to this process
        let current_process = GetCurrentProcess();
        let assign_result = AssignProcessToJobObject(job, current_process);

        if assign_result == 0 {
            // Assignment failed - this can happen if:
            // 1. The process is already in a job object
            // 2. The job object has incompatible restrictions
            // We still return success but log a warning, as the job object is created
            // and could be used for child processes
            CloseHandle(job);
            return Err(
                "Failed to assign current process to job object - process may already be in a job"
                    .to_string(),
            );
        }

        Ok(job)
    }
}

impl Drop for WindowsSandbox {
    fn drop(&mut self) {
        if let Some(handle) = self.job_handle {
            #[cfg(target_os = "windows")]
            // SAFETY: handle was obtained from CreateJobObjectW and is valid until closed.
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(handle);
            }
        }
    }
}
