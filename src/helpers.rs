
use crate::{globals::*, jobs::JobControl};
use std::{collections::HashMap, fs, path::PathBuf};
use libc;

pub fn get_history_file_path() -> PathBuf {
    let mut path: PathBuf = get_nash_dir();
    path.push("history");
    path
}

pub fn get_alias_file_path() -> PathBuf {
    let mut path: PathBuf = get_nash_dir();
    path.push("alias");
    path
}

pub fn load_aliases(path: &PathBuf) -> HashMap<String, String> {
    let mut aliases: HashMap<String, String> = HashMap::new();
    if let Ok(contents) = fs::read_to_string(path) {
        for line in contents.lines() {
            if let Some(pos) = line.find('=') {
                let (name, command) = line.split_at(pos);
                aliases.insert(name.trim().to_string(), command[1..].trim().to_string());
            }
        }
    }
    aliases
}

pub fn save_aliases(path: &PathBuf, aliases: &HashMap<String, String>) {
    let content: String = aliases
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(path, content).expect("Unable to write alias file");
}
pub fn parse_job_specifier(spec: &str, job_control: &JobControl) -> Result<libc::pid_t, String> {
    if spec.starts_with('%') {
        // Job number specified with %
        match spec[1..].parse::<usize>() {
            Ok(job_num) => {
                let jobs = job_control.list_jobs();
                if job_num < 1
                {
                    return std::result::Result::Err("No such job".to_owned());
                }
                jobs.get(job_num - 1)
                    .map(|job| job.pid)
                    .ok_or_else(|| "No such job".to_string())
            }
            Err(_) => Err("Invalid job number".to_string()),
        }
    } else {
        // Direct PID
        spec.parse::<libc::pid_t>()
            .map_err(|_| "Invalid process ID".to_string())
    }
}
