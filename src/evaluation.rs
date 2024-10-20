use crate::config::*;
use crate::globals::*;
use crate::commands::*;
use crate::helpers::*;
use crate::command_parsing::*;
use crate::jobs::{JobControl, RECEIVED_SIGTSTP, JobStatus};
use std::process::{self, Stdio, Command};
use std::{fs::OpenOptions, io::{Write, Error}, env, path::PathBuf, collections::HashMap, os::unix::process::CommandExt, sync::atomic::Ordering};

pub fn eval(state: &mut ShellState, conf: &mut Config, job_control: &mut JobControl, cmd: String, internal: bool) -> String {
    let chars_to_check: [char; 3] = [';', '|', '>'];

    if cmd.contains(|c| chars_to_check.contains(&c)) {
        return special_eval(state, conf, job_control, cmd);
    }

    let expanded_cmd: String = if cmd.starts_with('.') { lim_expand(&cmd) } else { expand(&cmd) };
    let cmd_parts: Vec<String> = split_command(&expanded_cmd);

    if cmd_parts.is_empty() {
        return "Empty command".to_owned();
    }

    // Check if the first part is an environment variable assignment
    if cmd_parts[0].contains('=') {
        return env_var_eval(job_control, cmd_parts[0].clone());
    }

    // Load aliases
    let alias_file_path: PathBuf = get_alias_file_path();
    let aliases: HashMap<String, String> = load_aliases(&alias_file_path);

    // Check for alias and expand if found
    let expanded_cmd_parts: Vec<String> = if let Some(alias_cmd) = aliases.get(&cmd_parts[0]) {
        let mut new_cmd_parts: Vec<String> = alias_cmd.split_whitespace().map(String::from).collect();
        new_cmd_parts.extend_from_slice(&cmd_parts[1..]);
        new_cmd_parts
    } else {
        cmd_parts
    };

    if expanded_cmd_parts[0].as_str().starts_with('.') {
        execute_file(&cmd[1..], &expanded_cmd_parts[1..])
    }
    else {
        match expanded_cmd_parts[0].as_str() {
            cmd if cmd.starts_with('.') => "This path should be unreachable.".to_owned(),
            "cd" => handle_cd(&expanded_cmd_parts),
            "history" => handle_history(&expanded_cmd_parts),
            "exit" => {
                println!("Exiting...");
                process::exit(0);
            }
            // TODO: Allow creation of aliases to files, IE alias kill='./pkill' and summonning of current directory files IE summon ./pkill
            "summon" => handle_summon(&expanded_cmd_parts),
            "alias" => handle_alias(&expanded_cmd_parts),
            "rmalias" => handle_remove_alias(&expanded_cmd_parts),
            "help" => show_help(),
            "set" => set_conf_rule(conf, &expanded_cmd_parts),
            "unset" => unset_conf_rule(conf, &expanded_cmd_parts),
            "rconf" => read_conf(conf, &expanded_cmd_parts),
            "reset" => reset(conf, get_nash_dir()),
            "fg" => handle_fg(&expanded_cmd_parts, job_control),
            "bg" => handle_bg(&expanded_cmd_parts, job_control),
            "jobs" => handle_jobs(job_control),
            "pwd" => env::current_dir().unwrap().to_str().unwrap().to_string(),
            _ => {
                // If not a built-in command, execute as an external command
                let result: String = execute_external_command(&expanded_cmd_parts[0], &expanded_cmd_parts, internal, job_control);
                if !result.is_empty() {
                    return format!("{}", result);
                }
                NO_RESULT.to_owned()
            }
        }
    }
}

pub fn special_eval(state: &mut ShellState, conf: &mut Config , job_control: &mut JobControl, cmd: String) -> String {
    let mut result: String = String::new();
    let commands: Vec<String> = cmd.split(';').map(|s| s.trim().to_owned()).collect();

    for command in commands {
        if command.contains("|") {
            result = pipe_eval(command);
        } else if command.contains(">") {
            result = out_redir_eval(state, conf, job_control, command);
        } else {
            result = eval(state, conf, job_control, command, false);
        }
    }
    result
}

pub fn pipe_eval(cmd: String) -> String {
    let parts: Vec<String> = cmd.split('|').map(|s| s.trim().to_owned()).collect();

    let mut input: String = String::new();
    for part in parts {
        let command_parts: Vec<String> = part.split_whitespace().map(String::from).collect();
        let command: &String = &command_parts[0];
        let args: &[String] = &command_parts[1..];

        // Create a command with the input as stdin
        let mut child: process::Child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to start command");

        // Write the previous command's output to this command's input
        if !input.is_empty() {
            let stdin: &mut process::ChildStdin = child.stdin.as_mut().expect("Failed to open stdin");
            stdin.write_all(input.as_bytes()).expect("Failed to write to stdin");
        }

        // Get the output of this command
        let output: process::Output = child.wait_with_output().expect("Failed to read stdout");

        if output.status.success() {
            input = String::from_utf8_lossy(&output.stdout).into_owned();
        } else {
            return format!("Command failed and output error: {}", String::from_utf8_lossy(&output.stderr));
        }
    }

    input
}

pub fn out_redir_eval(state: &mut ShellState, conf: &mut Config, job_control: &mut JobControl, cmd: String) -> String {
    let parts: Vec<String> = if cmd.contains("2>>") {
        cmd.splitn(2, "2>>").map(|s| s.trim().to_owned()).collect()
    } else if cmd.contains(">>") {
        cmd.splitn(2, ">>").map(|s| s.trim().to_owned()).collect()
    } else if cmd.contains("2>") {
        cmd.splitn(2, "2>").map(|s| s.trim().to_owned()).collect()
    } else {
        cmd.splitn(2, ">").map(|s| s.trim().to_owned()).collect()
    };

    if parts.len() != 2 {
        return "Invalid output redirection syntax".to_owned();
    }

    let command: String = parts[0].clone();
    let file_path: String = parts[1].clone();
    let append_mode: bool = cmd.contains(">>");

    let mut file_options: OpenOptions = OpenOptions::new();
    file_options.write(true).create(true);
    if append_mode {
        file_options.append(true);
    } else {
        file_options.truncate(true);
    }

    match file_options.open(&file_path) {
        Ok(mut file) => {
            let output: String = eval(state, conf, job_control, command, false);
            match file.write_all(output.as_bytes()) {
                Ok(_) => NO_RESULT.to_owned(),
                Err(e) => format!("Failed to write to file: {}", e),
            }
        }
        Err(e) => format!("Failed to open file: {}", e),
    }
}
pub fn execute_external_command(cmd: &str, cmd_parts: &[String], internal: bool, job_control: &mut JobControl) -> String {
    match find_command_in_path(cmd) {
        Some(path) => {
            let mut command = Command::new(path);
            if cmd_parts.len() > 1 {
                command.args(&cmd_parts[1..]);
            }

            command.process_group(0); // Create a new process group

            if internal {
                command.stdin(Stdio::null());
                command.stdout(Stdio::null());
                command.stderr(Stdio::null());
            } else {
                command.stdin(Stdio::inherit());
                command.stdout(Stdio::inherit());
                command.stderr(Stdio::inherit());
            }

            match command.spawn() {
                Ok(child) => {
                    let pid = child.id() as libc::pid_t;
                    let cmd_string = cmd_parts.join(" ");
                    job_control.add_job(pid, cmd_string.clone());

                    if !internal {
                        // Give terminal control to the child process group
                        unsafe {
                            libc::tcsetpgrp(libc::STDIN_FILENO, pid);
                        }

                        // Wait for the child process
                        let result = job_control.wait_for_job(pid);

                        // Always take back terminal control
                        unsafe {
                            libc::tcsetpgrp(libc::STDIN_FILENO, libc::getpgrp());
                        }

                        match result {
                            Ok(status) => {
                                match status {
                                    JobStatus::Done => {
                                        job_control.remove_job(pid);
                                        NO_RESULT.to_owned()
                                    },
                                    JobStatus::Stopped => {
                                        println!("Job stopped: {}", cmd_string);
                                        NO_RESULT.to_owned()
                                    },
                                    _ => format!("Unexpected job status: {:?}", status),
                                }
                            },
                            Err(e) => format!("Error waiting for command: {}", e),
                        }
                    } else {
                        NO_RESULT.to_owned()
                    }
                }
                Err(e) => format!("Failed to execute command: {}", e),
            }
        }
        None => format!("Command not found: {}", cmd),
    }
}

fn find_command_in_path(cmd: &str) -> Option<String> {
    if let Ok(path) = env::var("PATH") {
        for dir in path.split(":") {
            let full_path: String = format!("{}/{}", dir, cmd);
            if std::fs::metadata(&full_path).is_ok() {
                return Some(full_path);
            }
        }
    }
    None
}

pub fn env_var_eval(job_control: &mut JobControl, cmd: String) -> String {
    let count: usize = cmd.chars().filter(|c| *c == '=').count();
    if count > 1 {
        return "Command contains more than one environment variable assignment (parsing issue)"
            .to_owned();
    } else if count == 1 {
        // Handle environment variable assignment
        let parts: Vec<String> = cmd.split('=').map(|s| s.trim().to_owned()).collect();
        if parts.len() == 2 {
            env::set_var(&parts[0], &parts[1]);
            return NO_RESULT.to_owned();
        } else {
            return "Invalid environment variable assignment".to_owned();
        }
    }

    // Handle environment variable extraction with $
    if cmd.starts_with('$') {
        let var_name: &str = &cmd[1..];
        if var_name == "0" {
            return "nash".to_owned(); // Special case for $0
        }
        if let Ok(value) = env::var(var_name) {
            if cmd.trim() == format!("${}", var_name) {
                // If the command is just the variable, attempt to execute it
                return execute_external_command(&value, &[value.clone()], false, job_control);
            } else {
                // Otherwise, return the value
                return value;
            }
        } else {
            return format!("Environment variable not found: {}", var_name);
        }
    }

    // If we reach here, it means there was no assignment or extraction
    "Invalid environment variable operation".to_owned()
}
pub fn execute_file(path: &str, args: &[String]) -> String {
    let full_path: PathBuf = if path.starts_with('/') {
        PathBuf::from(path)
    } else {
        env::current_dir().unwrap_or(PathBuf::from("/")).join(path)
    };

    if full_path.is_file() {
        let output: Result<process::Output, Error> = Command::new(&full_path).args(args).output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    String::from_utf8_lossy(&output.stdout).into_owned()
                } else {
                    format!(
                        "Program failed and output error: {}",
                        String::from_utf8_lossy(&output.stderr)
                    )
                }
            }
            Err(e) => format!("Failed to execute file: {}", e),
        }
    } else {
        format!("File not found or not executable: {}", full_path.display())
    }
}
