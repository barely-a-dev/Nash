use crate::config::*;
use crate::globals::*;
use crate::commands::*;
use crate::command_parsing::*;
use crate::jobs::JobControl;
use std::process::{self, Stdio, Command};
use std::{fs::OpenOptions, io::{Write, Error}, env, path::PathBuf, os::unix::process::CommandExt};

pub fn eval(state: &mut ShellState, conf: &mut Config, job_control: &mut JobControl, cmd: String, internal: bool) -> String {
    let chars_to_check: [char; 3] = [';', '|', '>'];

    let expanded_cmd: String = if cmd.starts_with('.') { lim_expand(state, &cmd) } else { expand(state, &cmd) };
    let cmd_parts: Vec<String> = split_command(&expanded_cmd);

    if cmd_parts.is_empty() {
        return "Empty command".to_owned();
    }

    // Check if the first part is an environment variable assignment
    if cmd_parts[0].contains('=') {
        let parts: Vec<&str> = cmd_parts[0].splitn(2, '=').collect();
        if parts.len() == 2 {
            state.set_local_var(parts[0], parts[1]);
            return NO_RESULT.to_owned();
        } else {
            return "Invalid variable assignment".to_owned();
        }
    }

    let expanded_cmd_parts: Vec<String> = expand_aliases(cmd_parts);

    if cmd.contains(|c| chars_to_check.contains(&c)) {
        return special_eval(state, conf, job_control, expanded_cmd_parts.join(" "));
    }

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
            "summon" => handle_summon(&expanded_cmd_parts),
            "alias" => handle_alias(state, &expanded_cmd_parts),
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
            "settings" => handle_settings(conf, &expanded_cmd_parts),
            "TEST" => test_nash(conf, state, job_control, &expanded_cmd_parts),
            "setprompt" => cmd_set_prompt(&expanded_cmd_parts, state),
            "export" => export_env_var_eval(state, &expanded_cmd_parts),

            _ => {
                // If not a built-in command, execute as an external command
                let result: String = execute_external_command(&expanded_cmd_parts[0], &expanded_cmd_parts, internal, job_control);
                if !result.is_empty() {
                    return format!("{}", result);
                }
                NO_RESULT.to_owned()
            }
        }.trim().to_owned()
    }
}

pub fn special_eval(state: &mut ShellState, conf: &mut Config , job_control: &mut JobControl, cmd: String) -> String {
    let mut result: String = String::new();
    let commands: Vec<String> = cmd.split(';').map(|s| s.trim().to_owned()).collect();

    for command in commands {
        if command.contains("|") {
            result = pipe_eval(state, conf, job_control, command);
        } else if command.contains(">") {
            result = out_redir_eval(state, conf, job_control, command);
        } else {
            result = eval(state, conf, job_control, command, false);
        }
    }
    result
}

pub fn pipe_eval(state: &mut ShellState, conf: &mut Config, job_control: &mut JobControl, cmd: String) -> String {
    let parts: Vec<String> = cmd.split('|').map(|s| s.trim().to_owned()).collect();
    let mut input: String = String::new();

    for part in parts {
        let expanded_cmd: String = if part.starts_with('.') { lim_expand(state, &part) } else { expand(state, &part) };
        let cmd_parts: Vec<String> = split_command(&expanded_cmd);

        if cmd_parts.is_empty() {
            return "Empty command in pipe".to_owned();
        }

        // Check for environment variable assignment (unlikely in a pipe, but we'll handle it)
        if cmd_parts[0].contains('=') {
            return "Environment variable assignment not supported in pipes".to_owned();
        }

        let expanded_cmd_parts: Vec<String> = expand_aliases(cmd_parts);

        let result: String = if expanded_cmd_parts[0].as_str().starts_with('.') {
            execute_file(&part[1..], &expanded_cmd_parts[1..])
        } else {
            match expanded_cmd_parts[0].as_str() {
                "cd" => handle_cd(&expanded_cmd_parts),
                "history" => handle_history(&expanded_cmd_parts),
                "alias" => handle_alias(state, &expanded_cmd_parts),
                "rmalias" => handle_remove_alias(&expanded_cmd_parts),
                "help" => show_help(),
                "set" => set_conf_rule(conf, &expanded_cmd_parts),
                "unset" => unset_conf_rule(conf, &expanded_cmd_parts),
                "rconf" => read_conf(conf, &expanded_cmd_parts),
                "jobs" => handle_jobs(job_control),
                "pwd" => env::current_dir().unwrap().to_str().unwrap().to_string(),
                "settings" => handle_settings(conf, &expanded_cmd_parts),
                "setprompt" => cmd_set_prompt(&expanded_cmd_parts, state),
                "exit" | "reset" | "fg" | "bg" | "summon" | "export" => {
                    return format!("Command '{}' is not supported in pipes", expanded_cmd_parts[0]);
                }
                _ => {
                    // If not a built-in command, execute as an external command
                    let mut child: process::Child = match Command::new(&expanded_cmd_parts[0])
                        .args(&expanded_cmd_parts[1..])
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .spawn()
                    {
                        Ok(child) => child,
                        Err(e) => {
                            return format!("Failed to execute command: {}", e);
                        }
                    };

                    if !input.is_empty() {
                        match child.stdin.as_mut() {
                            Some(stdin) => {
                                if let Err(e) = stdin.write_all(input.as_bytes()) {
                                    return format!("Failed to write to stdin: {}", e);
                                }
                            },
                            None => {
                                return "Failed to open stdin".to_string();
                            }
                        }
                    }

                    let output: process::Output = child.wait_with_output().expect("Failed to read stdout");
                    if output.status.success() {
                        String::from_utf8_lossy(&output.stdout).into_owned()
                    } else {
                        return format!("Command failed: {}", String::from_utf8_lossy(&output.stderr));
                    }
                }
            }
        };

        input = result;
    }

    input
}
// TODO: Input redirection, actual error redirection
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

    let expanded_cmd: String = if command.starts_with('.') { lim_expand(state, &command) } else { expand(state, &command) };
    let cmd_parts: Vec<String> = split_command(&expanded_cmd);

    if cmd_parts.is_empty() {
        return "Empty command".to_owned();
    }

    // Check if the first part is an environment variable assignment
    if cmd_parts[0].contains('=') {
        return "Environment variable assignment not supported with output redirection".to_owned();
    }

    let expanded_cmd_parts: Vec<String> = expand_aliases(cmd_parts);

    let output: String = if expanded_cmd_parts[0].as_str().starts_with('.') {
        execute_file(&command[1..], &expanded_cmd_parts[1..])
    } else {
        match expanded_cmd_parts[0].as_str() {
            "cd" => handle_cd(&expanded_cmd_parts),
            "history" => handle_history(&expanded_cmd_parts),
            "alias" => handle_alias(state, &expanded_cmd_parts),
            "rmalias" => handle_remove_alias(&expanded_cmd_parts),
            "help" => show_help(),
            "set" => set_conf_rule(conf, &expanded_cmd_parts),
            "unset" => unset_conf_rule(conf, &expanded_cmd_parts),
            "rconf" => read_conf(conf, &expanded_cmd_parts),
            "jobs" => handle_jobs(job_control),
            "pwd" => env::current_dir().unwrap().to_str().unwrap().to_string(),
            "settings" => handle_settings(conf, &expanded_cmd_parts),
            "setprompt" => cmd_set_prompt(&expanded_cmd_parts, state),
            "exit" | "reset" | "fg" | "bg" | "summon" | "export" => {
                return format!("Command '{}' is not supported with output redirection", expanded_cmd_parts[0]);
            }
            _ => {
                // If not a built-in command, execute as an external command
                execute_external_command(&expanded_cmd_parts[0], &expanded_cmd_parts, true, job_control)
            }
        }
    };

    let mut file_options: OpenOptions = OpenOptions::new();
    file_options.write(true).create(true);
    if append_mode {
        file_options.append(true);
    } else {
        file_options.truncate(true);
    }

    match file_options.open(&file_path) {
        Ok(mut file) => {
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
            let mut command: Command = Command::new(path);
            if cmd_parts.len() > 1 {
                command.args(&cmd_parts[1..]);
            }

            command.process_group(0); // Create a new process group

            if internal {
                command.stdin(Stdio::null());
                command.stdout(Stdio::piped());
                command.stderr(Stdio::piped());
            } else {
                command.stdin(Stdio::inherit());
                command.stdout(Stdio::piped());
                command.stderr(Stdio::piped());
            }

            match command.spawn() {
                Ok(child) => {
                    let pid: i32 = child.id() as libc::pid_t;
                    let cmd_string: String = cmd_parts.join(" ");
                    job_control.add_job(pid, cmd_string.clone());

                    if !internal {
                        // Give terminal control to the child process group
                        unsafe {
                            libc::tcsetpgrp(libc::STDIN_FILENO, pid);
                        }
                    }

                    // Wait for the child process and capture its output
                    let output: process::Output = child.wait_with_output().expect("Failed to wait on child");

                    if !internal {
                        // Always take back terminal control
                        unsafe {
                            libc::tcsetpgrp(libc::STDIN_FILENO, libc::getpgrp());
                        }
                    }

                    let stdout: String = String::from_utf8_lossy(&output.stdout).into_owned();
                    let stderr: String = String::from_utf8_lossy(&output.stderr).into_owned();

                    if output.status.success() {
                        job_control.remove_job(pid);
                        if stdout.is_empty() && stderr.is_empty() {
                            NO_RESULT.to_owned()
                        } else {
                            format!("{}{}", stdout, stderr)
                        }
                    } else {
                        format!("Command failed: {}", stderr)
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

pub fn export_env_var_eval(state: &mut ShellState, cmd_parts: &[String]) -> String {
    if cmd_parts.len() < 2 {
        return "Usage: export <variable>=<value>".to_owned();
    }

    for part in &cmd_parts[1..] {
        if let Some(pos) = part.find('=') {
            let (name, value) = part.split_at(pos);
            let name: &str = name.trim();
            let value: &str = &value[1..].trim(); // Skip the '=' character

            if name.is_empty() || value.is_empty() {
                return "Invalid export syntax. Usage: export <variable>=<value>".to_owned();
            }

            // Move from local vars to environment
            if let Some(local_value) = state.local_vars.remove(name) {
                env::set_var(name, local_value);
            } else {
                env::set_var(name, value);
            }
        } else {
            // Handle exporting a variable without assignment
            let name = part.trim();
            if let Some(local_value) = state.local_vars.get(name) {
                env::set_var(name, local_value);
            } else if let Ok(env_value) = env::var(name) {
                env::set_var(name, env_value);
            } else {
                return format!("Variable '{}' not found", name);
            }
        }
    }

    NO_RESULT.to_owned()
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
