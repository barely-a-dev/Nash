use libc::{pid_t, SIGCONT, SIGTSTP};
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
    pub jobs: HashMap<pid_t, Job>,
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

    pub fn resume_job(&mut self, pid: libc::pid_t, foreground: bool) -> Result<()> {
        let job_count = self.jobs.len();
        if let Some(job) = self.jobs.get_mut(&pid) {
            unsafe {
                // Continue the process
                if libc::kill(-pid, libc::SIGCONT) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
    
                if foreground {
                    // Give terminal control to the process group
                    if libc::tcsetpgrp(libc::STDIN_FILENO, pid) == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
    
                    // Wait for the job
                    let mut status: libc::c_int = 0;
                    if libc::waitpid(pid, &mut status, libc::WUNTRACED) == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
    
                    // Take back terminal control
                    let shell_pgid = libc::getpgrp();
                    if libc::tcsetpgrp(libc::STDIN_FILENO, shell_pgid) == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
    
                    if libc::WIFSTOPPED(status) {
                        job.status = JobStatus::Stopped;
                        println!("\n[{}]+  Stopped                 {}",job_count, job.command);
                    } else {
                        self.remove_job(pid);
                    }
                } else {
                    job.status = JobStatus::Running;
                }
            }
            
            self.current_job = Some(pid);
            Ok(())
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "Job not found"))
        }
    }    

    pub fn wait_for_job(&mut self, pid: pid_t) -> Result<JobStatus> {
        let mut status: i32 = 0;
        loop {
            let wait_result = unsafe { libc::waitpid(pid, &mut status, libc::WUNTRACED) };
            if wait_result == -1 {
                return Err(Error::last_os_error());
            }

            if libc::WIFSTOPPED(status) {
                if let Some(job) = self.jobs.get_mut(&pid) {
                    job.status = JobStatus::Stopped;
                }
                return Ok(JobStatus::Stopped);
            } else if libc::WIFEXITED(status) || libc::WIFSIGNALED(status) {
                if let Some(job) = self.jobs.get_mut(&pid) {
                    job.status = JobStatus::Done;
                }
                return Ok(JobStatus::Done);
            }

            // Check if we received SIGTSTP
            if RECEIVED_SIGTSTP.load(Ordering::SeqCst) {
                RECEIVED_SIGTSTP.store(false, Ordering::SeqCst);
                if let Some(job) = self.jobs.get_mut(&pid) {
                    job.status = JobStatus::Stopped;
                }
                unsafe {
                    libc::kill(-pid, libc::SIGTSTP);
                }
                return Ok(JobStatus::Stopped);
            }
        }
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
    pub fn foreground_job(&mut self, pid: pid_t) -> Result<JobStatus> {
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