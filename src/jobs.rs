use libc::{pid_t, SIGCONT, SIGTSTP, WUNTRACED};
use std::collections::HashMap;
use std::io::{Error, Result};
use nix::sys::signal::{self, SigAction, SigHandler, Signal};
use std::sync::atomic::{AtomicBool, Ordering};

pub static RECEIVED_SIGTSTP: AtomicBool = AtomicBool::new(false);

pub fn setup_signal_handlers() -> std::result::Result<(), nix::Error> {
    // Setup SIGTSTP (Ctrl+Z) handler
    unsafe {
        let sigtstp_action: SigAction = SigAction::new(
            SigHandler::Handler(handle_sigtstp),
            signal::SaFlags::empty(),
            signal::SigSet::empty(),
        );
        signal::sigaction(Signal::SIGTSTP, &sigtstp_action)?;
    }

    // Ignore SIGTTOU and SIGTTIN to prevent the shell from stopping
    // when it tries to access the terminal
    unsafe {
        let sig_ign: SigAction = SigAction::new(
            SigHandler::SigIgn,
            signal::SaFlags::empty(),
            signal::SigSet::empty(),
        );
        signal::sigaction(Signal::SIGTTOU, &sig_ign)?;
        signal::sigaction(Signal::SIGTTIN, &sig_ign)?;
    }

    Ok(())
}

pub extern "C" fn handle_sigtstp(_: i32) {
    RECEIVED_SIGTSTP.store(true, Ordering::SeqCst);
    unsafe {
        libc::kill(0, libc::SIGTSTP); // Send SIGTSTP to the entire process group
    }
}

#[derive(Debug, Clone)]
pub struct Job {
    pub pid: pid_t,
    pub command: String,
    pub status: JobStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JobStatus {
    Running,
    Stopped,
    Done,
}

pub struct JobControl {
    jobs: HashMap<pid_t, Job>,
    current_job: Option<pid_t>,
}

impl JobControl {
    pub fn new() -> Self {
        JobControl {
            jobs: HashMap::new(),
            current_job: None,
        }
    }

    /// Add a new job to the job list
    pub fn add_job(&mut self, pid: pid_t, command: String) {
        let job = Job {
            pid,
            command,
            status: JobStatus::Running,
        };
        self.jobs.insert(pid, job);
        self.current_job = Some(pid);
    }

    pub fn remove_job(&mut self, pid: pid_t) {
        self.jobs.remove(&pid);
        if Some(pid) == self.current_job {
            self.current_job = None;
        }
    }

    /// Stop a running job
    pub fn stop_job(&mut self, pid: pid_t) -> Result<()> {
        if let Some(job) = self.jobs.get_mut(&pid) {
            unsafe {
                if libc::kill(-pid, SIGTSTP) == -1 {
                    return Err(Error::last_os_error());
                }
            }
            job.status = JobStatus::Stopped;
            Ok(())
        } else {
            Err(Error::new(std::io::ErrorKind::NotFound, "Job not found"))
        }
    }    

    /// Continue a stopped job
    pub fn continue_job(&mut self, pid: pid_t) -> Result<()> {
        if let Some(job) = self.jobs.get_mut(&pid) {
            unsafe {
                if libc::kill(pid, SIGCONT) == -1 {
                    return Err(Error::last_os_error());
                }
            }
            job.status = JobStatus::Running;
            self.current_job = Some(pid);
            Ok(())
        } else {
            Err(Error::new(
                std::io::ErrorKind::NotFound,
                "Job not found",
            ))
        }
    }

    /// Wait for job status changes
    pub fn wait_for_job(&mut self, pid: pid_t) -> Result<()> {
        let mut status: i32 = 0;
        unsafe {
            if libc::waitpid(pid, &mut status, WUNTRACED) == -1 {
                return Err(Error::last_os_error());
            }
        }

        if let Some(job) = self.jobs.get_mut(&pid) {
            // Update job status based on wait result
            if libc::WIFSTOPPED(status) {
                job.status = JobStatus::Stopped;
            } else if libc::WIFEXITED(status) || libc::WIFSIGNALED(status) {
                job.status = JobStatus::Done;
                self.current_job = None;
            }
        }

        Ok(())
    }

    /// Remove completed jobs from the job list
    pub fn cleanup_jobs(&mut self) {
        self.jobs.retain(|_, job| job.status != JobStatus::Done);
    }

    /// List all jobs
    pub fn list_jobs(&self) -> Vec<&Job> {
        self.jobs.values().collect()
    }

    /// Get the current (most recently used) job
    pub fn get_current_job(&self) -> Option<&Job> {
        self.current_job.and_then(|pid| self.jobs.get(&pid))
    }

    /// Bring a job to the foreground
    pub fn foreground_job(&mut self, pid: pid_t) -> Result<()> {
        // First continue the job if it was stopped
        if let Some(job) = self.jobs.get(&pid) {
            if job.status == JobStatus::Stopped {
                self.continue_job(pid)?;
            }
        }

        // Set as current job and wait for it
        self.current_job = Some(pid);
        self.wait_for_job(pid)
    }

    /// Send a job to the background
    pub fn background_job(&mut self, pid: pid_t) -> Result<()> {
        if let Some(job) = self.jobs.get(&pid) {
            if job.status == JobStatus::Stopped {
                self.continue_job(pid)?;
            }
            Ok(())
        } else {
            Err(Error::new(
                std::io::ErrorKind::NotFound,
                "Job not found",
            ))
        }
    }
}

// Helper function to create a new process group
pub fn create_process_group(pid: pid_t) -> Result<()> {
    unsafe {
        if libc::setpgid(pid, pid) == -1 {
            return Err(Error::last_os_error());
        }
    }
    Ok(())
}

// Helper function to give terminal control to a process group
pub fn give_terminal_to(pid: pid_t) -> Result<()> {
    unsafe {
        if libc::tcsetpgrp(libc::STDIN_FILENO, pid) == -1 {
            return Err(Error::last_os_error());
        }
    }
    Ok(())
}