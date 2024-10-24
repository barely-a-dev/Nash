use std::fs::File;
use std::io::{BufReader, BufRead};
use std::path::Path;
use std::sync::atomic::Ordering;
use libc;
use crate::{eval, Config, ShellState, RECEIVED_SIGTSTP, JobControl};

pub struct ScriptExecutor<'a> {
    state: &'a mut ShellState,
    conf: &'a mut Config,
    job_control: &'a mut JobControl,
}

impl<'a> ScriptExecutor<'a> {
    pub fn new(
        state: &'a mut ShellState,
        conf: &'a mut Config,
        job_control: &'a mut JobControl,
    ) -> Self {
        ScriptExecutor {
            state,
            conf,
            job_control,
        }
    }

    pub fn execute_script(&mut self, script_path: &Path) -> Result<(), std::io::Error> {
        let file: File = File::open(script_path)?;
        let reader: BufReader<File> = BufReader::new(file);
        let mut lines: Vec<String> = reader.lines().collect::<Result<_, _>>()?;
    
        // Skip shebang line if present
        if let Some(first_line) = lines.first() {
            if first_line.starts_with("#!") {
                lines.remove(0);
            }
        }
    
        // Create a new process group for the script
        unsafe {
            let script_pgid: i32 = libc::getpid();
            if libc::setpgid(0, script_pgid) == -1 {
                eprintln!("Warning: Failed to create new process group for script");
            }
        }
    
        // Execute all commands in the script
        for line in lines {
            let trimmed: &str = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                // Take terminal control before executing each command
                unsafe {
                    let script_pgid: i32 = libc::getpgrp();
                    if libc::tcsetpgrp(libc::STDIN_FILENO, script_pgid) == -1 {
                        eprintln!("Warning: Failed to take terminal control");
                    }
                }
    
                // Execute the command and handle output immediately
                let output: String = eval(self.state, self.conf, self.job_control, line.to_string(), true).trim().to_string();
                
                // If there's any output, print it
                if !output.is_empty() {
                    println!("{}", output);
                }
    
                // Check for job control signals
                if RECEIVED_SIGTSTP.load(Ordering::SeqCst) {
                    RECEIVED_SIGTSTP.store(false, Ordering::SeqCst);
                    if let Some(job) = self.job_control.get_current_job() {
                        println!("\n[{}] Stopped    {}", job.pid, job.command);
                        self.job_control.stop_job(job.pid)?;
                    }
                }
    
                // Clean up any completed jobs
                self.job_control.cleanup_jobs();
            }
        }
    
        // Return terminal control to the shell
        unsafe {
            let shell_pgid = libc::getpgrp();
            if libc::tcsetpgrp(libc::STDIN_FILENO, shell_pgid) == -1 {
                eprintln!("Warning: Failed to return terminal control to shell");
            }
        }
    
        Ok(())
    }    
}
