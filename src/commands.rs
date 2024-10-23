use crate::globals::*;
use crate::helpers::*;
use crate::jobs::JobControl;
use std::env;
use std::{path::PathBuf, fs::{self, remove_file, File}, io::Error, collections::HashMap, process::{self, Stdio, Command}};
use crate::arguments::*;
use crate::config::*;
use crate::jobs::JobStatus;
use crate::command_parsing::expand;

pub fn reset(conf: &mut Config, nash_dir: PathBuf) -> String
{
    conf.rules = HashMap::new();
    conf.temp_rules = HashMap::new();
    env::set_current_dir("/").unwrap();

    let mut output = String::new();

    match conf.get_rule("delete_on_reset", true) {
        None => {
            match File::create(nash_dir.join("config")) {
                Ok(_) => output.push_str("Successfully erased config.\n"),
                Err(e) => output.push_str(&format!("Could not erase config. Error: {}\n", e))
            }
            match File::create(nash_dir.join("history")) {
                Ok(_) => output.push_str("Successfully erased history.\n"),
                Err(e) => output.push_str(&format!("Could not erase history. Error: {}\n", e))
            }
            match File::create(nash_dir.join("alias")) {
                Ok(_) => output.push_str("Successfully erased aliases.\n"),
                Err(e) => output.push_str(&format!("Could not erase aliases. Error: {}\n", e))
            }
        }
        Some(v) => {
            if v.parse::<bool>().unwrap_or(false) {
                let nash_dir_disp: String = nash_dir.display().to_string();
                match remove_file("/usr/bin/nash") {
                    Ok(_) => output.push_str("Successfully deleted /usr/bin/nash.\n"),
                    Err(e) => output.push_str(&format!("Could not delete /usr/bin/nash file. Error: {}\n", e))
                }
                match fs::remove_dir_all(&nash_dir) {
                    Ok(_) => output.push_str(&format!("Successfully deleted {}.\n", nash_dir_disp)),
                    Err(e) => output.push_str(&format!("Could not delete {} directory. Error: {}\n", nash_dir_disp, e))
                }
            }            
        }
    }
    output.push_str("Exiting...\n");
    output
}

pub fn show_help() -> String {
    "cd <directory>: Change the current directory\n\
     history [--size|s] [--clear|c]: Display command history\n\
     exit: Exit the shell\n\
     summon [-w] <command>: Open an *external* command in a new terminal window\n\
     alias <identifier>[=original]: Create an alias for a command\n\
     rmalias <identifier>: Remove an alias for a command\n\
     help: Display this help menu\n\
     set <<<option> <value>>/<flag>>: Set a config rule to true or value\n\
     unset <option> <temp(bool)>: Unset a config rule (unimplemented)\n\
     reset: Reset the application, erase if delete_on_reset rule is true\n\
     rconf <option> [temp(bool)]: Read the value of a config rule (unimplemented)\n\
     settings: Display a simple config menu".to_owned()
}

pub fn handle_settings(conf: &mut Config, cmd_parts: &[String]) -> String {
    let (_, flag_args) = parse_args(cmd_parts);
    let temp: bool = flag_args.contains_key("temp") || flag_args.contains_key("t");
    let settings: Vec<GUIEntry> = vec![
        GUIEntry::new("error", "bool", &conf.get_rule("error", false).unwrap_or("false")),
        GUIEntry::new("delete_on_reset", "bool", &conf.get_rule("delete_on_reset", false).unwrap_or("false")),
    ];

    let mut menu: GUIMenu = GUIMenu::new("Nash settings".to_string(), settings);
    
    match menu.run() {
        Ok(_) => {
            for entry in menu.entries.iter() {
                conf.set_rule(&entry.name, &entry.value, false);
            }
            if !temp
            {
                conf.save_rules();
            }
            "Settings updated successfully".to_owned()
        }
        Err(e) => format!("Error in settings menu: {}", e),
    }
}
pub fn handle_summon(cmd_parts: &[String]) -> String {
    let (main_args, flag_args) = parse_summon_args(cmd_parts);
    let wait_for_exit: bool = flag_args.contains_key("w");
    let mut output = String::new();
    
    if main_args.is_empty() {
        return "Usage: summon [-w] <command>".to_owned();
    }

    let executable: &String = &main_args[0];

    // List of common terminal emulators
    let terminals: Vec<&str> = vec![
        "x-terminal-emulator", "gnome-terminal", "konsole", "xterm", "urxvt", "alacritty",
        "warp", "termux", "qterminal", "kitty", "tilix", "terminator", "rxvt", "st",
        "terminology", "hyper", "iterm2",
    ];

    let mut installed_terminals: Vec<&str> = Vec::new();

    // Check for installed terminals
    for &terminal in &terminals {
        if let Ok(term_output) = Command::new("which").arg(terminal).output() {
            if !term_output.stdout.is_empty() {
                installed_terminals.push(terminal);
                output.push_str(&format!("Found terminal: {}\n", terminal));
            }
        }
    }

    // No terminal :(
    if installed_terminals.is_empty() {
        return "Unable to find a suitable terminal emulator".to_owned();
    }

    // Use the first installed terminal in the list
    let terminal: &str = &installed_terminals[0];
    output.push_str(&format!("Using terminal: {}\n", terminal));

    let result: Result<process::Child, Error> = match terminal {
        "gnome-terminal" => {
            let mut cmd: Vec<&str> = vec!["bash", "-c"];
            cmd.extend(main_args.iter().map(|s| s.as_str()));
            output.push_str(&format!("Executing: {} -- {} {:?}\n", terminal, cmd.join(" "), cmd));
            Command::new(terminal)
                .args(&["--"])
                .args(&cmd)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        },
        "warp" => {
            output.push_str(&format!("Executing: {} --cmd {} {:?}\n", terminal, main_args.join(" "), main_args));
            Command::new(terminal)
                .arg("--cmd")
                .args(&main_args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        },
        "termux" => {
            output.push_str(&format!("Executing: {} -e {} {:?}\n", terminal, main_args.join(" "), main_args));
            Command::new(terminal)
                .arg("-e")
                .args(&main_args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        },
        _ => {
            output.push_str(&format!("Executing: {} -e {} {:?}\n", terminal, main_args.join(" "), main_args));
            Command::new(terminal)
                .arg("-e")
                .args(&main_args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        },
    };

    match result {
        Ok(mut child) => {
            if wait_for_exit {
                match child.wait() {
                    Ok(status) => output.push_str(&format!("Process exited with status: {}\n", status)),
                    Err(e) => output.push_str(&format!("Error waiting for process: {}\n", e)),
                }
            } else {
                output.push_str(&format!("Process started with PID: {}\n", child.id()))
            }
        },
        Err(e) => output.push_str(&format!("An error occurred: {} (Command: {})\n", e, executable)),
    }

    output
}

pub fn handle_history(cmd: &[String]) -> String {
    let (_, flag_args) = parse_args(cmd);
    let size: bool = flag_args.contains_key("size") || flag_args.contains_key("s");
    let clear: bool = flag_args.contains_key("clear") || flag_args.contains_key("c");
    let mut output: String = String::new();

    if !size && !clear {
        let history_file: PathBuf = get_history_file_path();
        match fs::read_to_string(history_file) {
            Ok(contents) => {
                for (i, line) in contents.lines().enumerate() {
                    output.push_str(&format!("{}: {}\n", i + 1, line));
                }
            }
            Err(e) => output.push_str(&format!("Failed to read history: {}\n", e)),
        }
    } else {
        if size {
            output.push_str(&format!("History file size: {}\n", get_history_file_path().metadata().unwrap().len()));
        }
        if clear {
            match File::create(get_history_file_path()) {
                Ok(_) => output.push_str("Successfully cleared history\n"),
                Err(e) => output.push_str(&format!("Could not clear history. Received error: {}\n", e))
            };
        }
    }
    output
}

pub fn handle_alias(cmd_parts: &[String]) -> String {
    let alias_file_path: PathBuf = get_alias_file_path();
    let mut aliases: HashMap<String, String> = load_aliases(&alias_file_path);

    if cmd_parts.len() == 1 {
        // List all aliases
        if aliases.is_empty() {
            return "No aliases defined.".to_owned();
        }
        return aliases.iter()
            .map(|(k, v)| format!("alias {}='{}'", k, v))
            .collect::<Vec<String>>()
            .join("\n");
    } else {
        let alias_str: String = cmd_parts[1..].join(" ");
        if let Some(pos) = alias_str.find('=') {
            let (name, command) = alias_str.split_at(pos);
            let name: &str = name.trim();
            let command: &str = &expand(command[1..].trim().trim_matches('\'').trim_matches('"'));
            aliases.insert(name.to_string(), command.to_string());
            save_aliases(&alias_file_path, &aliases);
            return format!("Alias '{}' created.", name);
        } else {
            // If no '=' is found, treat it as a query for a specific alias
            if let Some(command) = aliases.get(&alias_str) {
                return format!("alias {}='{}'", alias_str, command);
            } else {
                return format!("Alias '{}' not found.", alias_str);
            }
        }
    }
}

pub fn handle_remove_alias(cmd_parts: &[String]) -> String {
    if cmd_parts.len() != 2 {
        return "Usage: rmalias <alias_name>".to_owned();
    }

    let alias_name: &String = &cmd_parts[1];
    let alias_file_path: PathBuf = get_alias_file_path();
    let mut aliases: HashMap<String, String> = load_aliases(&alias_file_path);

    if aliases.remove(alias_name).is_some() {
        save_aliases(&alias_file_path, &aliases);
        format!("Alias '{}' removed.", alias_name)
    } else {
        format!("Alias '{}' not found.", alias_name)
    }
}

pub fn handle_cd(cmd_parts: &[String]) -> String {
    match cmd_parts.len() {
        1 => {
            "No directory passed. Usage: cd <directory>".to_owned()
        }
        2 => {
            let new_path: PathBuf = if cmd_parts[1].starts_with('/') {
                PathBuf::from(&cmd_parts[1])
            } else {
                env::current_dir().unwrap_or(PathBuf::from("/")).join(&cmd_parts[1])
            };

            if new_path.is_dir() {
                // Canonicalize the path to resolve any ".." or "." components
                match new_path.canonicalize() {
                    Ok(canonical_path) => {
                        match env::set_current_dir(canonical_path.to_string_lossy().into_owned())
                        {
                            Ok(_) => NO_RESULT.to_owned(),
                            Err(e) => return format!("Error setting cwd: {}", e)
                        };
                        NO_RESULT.to_owned()
                    }
                    Err(e) => format!("Error resolving path: {}", e),
                }
            } else {
                format!("Directory not found: {}", new_path.display())
            }
        }
        _ => "Usage: cd [path]".to_owned(),
    }
}

pub fn handle_fg(cmd: &[String], job_control: &mut JobControl) -> String {
    let (main_args, _) = parse_args(cmd);
    
    let job: libc::pid_t = if main_args.is_empty() {
        match job_control.get_current_job() {
            Some(job) => job.pid,
            None => return "No current job".to_string(),
        }
    } else {
        match parse_job_specifier(&main_args[0], job_control) {
            Ok(pid) => pid,
            Err(e) => return e,
        }
    };

    match job_control.resume_job(job, true) {
        Ok(_) => NO_RESULT.to_string(),
        Err(e) => format!("Could not bring job to foreground: {}", e),
    }
}

pub fn handle_bg(cmd: &[String], job_control: &mut JobControl) -> String {
    let (main_args, _) = parse_args(cmd);
    
    let job: i32 = if main_args.is_empty() {
        match job_control.get_current_job() {
            Some(job) => job.pid,
            None => return "No current job".to_string(),
        }
    } else {
        match parse_job_specifier(&main_args[0], job_control) {
            Ok(pid) => pid,
            Err(e) => return e,
        }
    };

    match job_control.resume_job(job, false) {
        Ok(_) => NO_RESULT.to_string(),
        Err(e) => format!("Could not continue job in background: {}", e),
    }
}

/// Handle the 'jobs' command
pub fn handle_jobs(job_control: &mut JobControl) -> String {
    // Get list of all jobs
    let jobs: Vec<&crate::jobs::Job> = job_control.list_jobs();
    
    if jobs.is_empty() {
        return "No jobs".to_string();
    }

    let mut output: String = String::new();
    
    // Format job listing
    for job in jobs {
        let current_marker = if Some(job.pid) == job_control.get_current_job().map(|j| j.pid) {
            "+"
        } else {
            "-"
        };

        let status_str = match job.status {
            JobStatus::Running => "Running",
            JobStatus::Stopped => "Stopped",
            JobStatus::Done => "Done",
        };

        output.push_str(&format!(
            "[{}]{} {} {}: {}\n",
            job.pid, current_marker, status_str, job.pid, job.command
        ));
    }

    output
}
