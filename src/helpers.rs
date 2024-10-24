
use crate::{globals::*, jobs::JobControl};
use std::{collections::HashMap, fs, path::PathBuf, env};
use libc;
use chrono;

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

pub fn parse_prompt(format: &str, state: &ShellState) -> String {
    let mut result: String = String::new();
    let mut chars: std::iter::Peekable<std::str::Chars<'_>> = format.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('u') => result.push_str(&state.username),
                Some('h') => result.push_str(&state.hostname),
                Some('w') => result.push_str(&env::current_dir().unwrap_or(PathBuf::from("/")).display().to_string()),
                Some('d') => result.push_str(&chrono::Local::now().format("%Y-%m-%d").to_string()),
                Some('t') => result.push_str(&chrono::Local::now().format("%H:%M:%S").to_string()),
                Some('\\') => result.push('\\'),
                Some(c) => {
                    result.push('\\');
                    result.push(c);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }

    result
}

pub fn read_prompt_from_file() -> Option<String> {
    let nash_dir = get_nash_dir();
    let prompt_file = nash_dir.join("prompt");

    match fs::read_to_string(prompt_file) {
        Ok(content) => {
            // Extract the prompt format from the PS1 assignment
            content.strip_prefix("PS1=\"").and_then(|s| s.strip_suffix("\""))
                .map(|s| s.to_string())
        },
        Err(_) => None,
    }
}

pub fn parse_job_specifier(spec: &str, job_control: &JobControl) -> Result<libc::pid_t, String> {
    if spec.starts_with('%') {
        // Job number specified with %
        match spec[1..].parse::<usize>() {
            Ok(job_num) => {
                let jobs: Vec<&crate::jobs::Job> = job_control.list_jobs();
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
