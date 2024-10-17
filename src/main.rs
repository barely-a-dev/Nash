// Important TODOs: Fix updating, which evidently never worked.
// MAJOR TODOs: export for env vars, CONFIG, set/unset for setting temp config (easy?), quotes and escaping ('', "", \), wildcards/regex (*, ?, [])
// Absolutely HUGE TODOs: Scripting (if, elif, else, fi, for, while, funcs, variables).
pub mod editing;
pub mod config;

use crate::editing::*;
use crate::config::*;
use chrono::{DateTime, Local};
use dirs;
use git2::Repository;
use libc::{getgrgid, getpwuid};
use rustyline::{
    completion::{Completer, Pair},
    error::ReadlineError,
    highlight::Highlighter,
    hint::Hinter,
    validate::{MatchingBracketValidator, ValidationContext, ValidationResult, Validator},
    Context, Editor,
};
use rustyline_derive::Helper;
use std::process::Child;
use std::{
    ptr,
    borrow::Cow,
    collections::HashMap,
    env,
    ffi::{OsStr, CStr},
    fs::{self, remove_file, File, OpenOptions},
    io::{self, Error, Write},
    os::unix::fs::{PermissionsExt, MetadataExt},
    path::{Path, PathBuf},
    process::{self, exit, Command, Stdio},
    time::SystemTime
};
use tokio::runtime::Runtime;
use whoami::fallible;
use console::{Style, Color};

const NO_RESULT: &str = "";

struct ShellState {
    cwd: String,
    username: String,
    hostname: String,
}
fn main() {
    let runtime: Runtime = Runtime::new().unwrap();
    runtime.block_on(async {
        let args: Vec<String> = std::env::args().collect();
        env::set_var("0", "nash");
        let mut conf: Config = match Config::new()
        {
            Ok(c) => c,
            Err(_) => {eprintln!("An error occurred when initializing the config."); exit(1)}
        };
        let mut state: ShellState = ShellState {
            cwd: std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("/"))
                .to_string_lossy()
                .to_string(),
            hostname: fallible::hostname().unwrap(),
            username: fallible::username().unwrap(),
        };
        env::set_var("IV", eval(&mut state, &mut conf, "env".to_owned()));
        if args.len() <= 1 {
            let _ = clear_screen();
            repl(&mut state, &mut conf);
        } else {
            handle_nash_args(&mut conf, args).await;
        }
    });
}

fn clear_screen() -> io::Result<Child> {
    Command::new("clear").spawn()
}

fn parse_args(args: &[String]) -> (Vec<String>, HashMap<String, Option<String>>) {
    let value_args: Vec<String> = Vec::from(["".to_owned()]);
    let mut parsed_args: HashMap<String, Option<String>> = HashMap::new();
    let mut non_flag_args: Vec<String> = Vec::new();
    let mut i: usize = 1; // Start from 1 to skip the program name

    while i < args.len() {
        let arg: &String = &args[i];
        
        if arg.starts_with("--") {
            // Long option
            let option: String = arg[2..].to_string();
            if i + 1 < args.len() && !args[i + 1].starts_with('-') && value_args.contains(&args[i + 1]) {
                // Option with value
                parsed_args.insert(option, Some(args[i + 1].clone()));
                i += 2;
            } else {
                // Flag option
                parsed_args.insert(option, None);
                i += 1;
            }
        } else if arg.starts_with('-') {
            // Short option
            let options: Vec<char> = arg[1..].chars().collect();
            for (j, opt) in options.iter().enumerate() {
                let option: String = opt.to_string();
                if j == options.len() - 1 && i + 1 < args.len() && !args[i + 1].starts_with('-') && value_args.contains(&args[i + 1]) {
                    // Last option in group with value
                    parsed_args.insert(option, Some(args[i + 1].clone()));
                    i += 1;
                } else {
                    // Flag option
                    parsed_args.insert(option, None);
                }
            }
            i += 1;
        } else {
            // Non-option argument
            non_flag_args.push(arg.clone());
            i += 1;
        }
    }

    (non_flag_args, parsed_args)
}

#[derive(Helper)]
struct ShellHelper {
    completer: AutoCompleter,
    highlighter: LineHighlighter,
    hinter: CommandHinter,
    validator: MatchingBracketValidator,
}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        self.completer.complete(line, pos, ctx)
    }
}

impl Validator for ShellHelper {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        self.validator.validate(ctx)
    }
}

impl Hinter for ShellHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<String> {
        self.hinter.hint(line, pos, ctx)
    }
}

impl Highlighter for ShellHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        default: bool,
    ) -> Cow<'b, str> {
        self.highlighter.highlight_prompt(prompt, default)
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        self.highlighter.highlight_hint(hint)
    }

    fn highlight<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
        self.highlighter.highlight(line, pos)
    }

    fn highlight_char(&self, line: &str, pos: usize) -> bool {
        self.highlighter.highlight_char(line, pos)
    }
}

fn repl(state: &mut ShellState, conf: &mut Config) {
    let history_file: PathBuf = get_history_file_path();
    let helper: ShellHelper = ShellHelper {
        completer: AutoCompleter::new(PathBuf::from(&state.cwd)),
        highlighter: LineHighlighter::new(),
        hinter: CommandHinter::new(),
        validator: MatchingBracketValidator::new(),
    };
    let mut rl: Editor<ShellHelper> = Editor::new();
    rl.set_helper(Some(helper));

    if rl.load_history(&history_file).is_err() {
        println!("No previous history.");
    }

    loop {
        let prompt: String = format!("[{}@{} {}]> ", state.username, state.hostname, state.cwd);
        match rl.readline(&prompt) {
            Ok(line) => {
                rl.add_history_entry(line.as_str());
                rl.save_history(&history_file).unwrap();
                let result: String = eval(state, conf, line);
                print(result);

                // Update the current directory in the AutoCompleter
                if let Some(helper) = rl.helper_mut() {
                    helper
                        .completer
                        .update_current_dir(PathBuf::from(&state.cwd));
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
    conf.save_rules();
}

fn get_nash_dir() -> PathBuf {
    let mut path: PathBuf = dirs::home_dir().expect("Unable to get home directory");
    path.push(".nash");
    fs::create_dir_all(&path).expect("Unable to create .nash directory");
    path
}

fn get_history_file_path() -> PathBuf {
    let mut path: PathBuf = get_nash_dir();
    path.push("history");
    path
}

fn get_alias_file_path() -> PathBuf {
    let mut path: PathBuf = get_nash_dir();
    path.push("alias");
    path
}

fn eval(state: &mut ShellState, conf: &mut Config, cmd: String) -> String {
    let chars_to_check: [char; 3] = [';', '|', '>'];

    if cmd.contains(|c| chars_to_check.contains(&c)) {
        return special_eval(state, conf, cmd);
    }

    let expanded_cmd: String = expand(state, &cmd);
    let cmd_parts: Vec<String> = split_command(&expanded_cmd);

    if cmd_parts.is_empty() {
        return "Empty command".to_owned();
    }

    // Check if the first part is an environment variable assignment
    if cmd_parts[0].contains('=') {
        return env_var_eval(state, cmd_parts[0].clone());
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

    // Now process the command (which might be an expanded alias)
    match expanded_cmd_parts[0].as_str() {
        cmd if cmd.starts_with('.') => execute_file(state, &cmd[1..], &expanded_cmd_parts[1..]),
        "cp" => handle_cp(&expanded_cmd_parts),
        "mv" => handle_mv(&expanded_cmd_parts),
        "rm" => handle_rm(&expanded_cmd_parts),
        "mkdir" => handle_mkdir(&expanded_cmd_parts),
        "ls" => handle_ls(state, &expanded_cmd_parts),
        "cd" => handle_cd(state, &expanded_cmd_parts),
        "history" => handle_history(),
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
        "reset" => reset(conf, state, get_nash_dir()),
        "pwd" => state.cwd.clone(),
        _ => {
            // If not a built-in command, execute as an external command
            let result: String = execute_external_command(&expanded_cmd_parts[0], &expanded_cmd_parts);
            if !result.is_empty() {
                return format!("{}", result);
            }
            NO_RESULT.to_owned()
        }
    }
}

fn reset(conf: &mut Config, state: &mut ShellState, nash_dir: PathBuf) -> String
{
    conf.rules = HashMap::new();
    conf.temp_rules = HashMap::new();
    state.cwd = "/".to_owned();

    match conf.get_rule("delete_on_reset", true)
    {
        None =>
        {
            match File::create(nash_dir.join("config"))
            {
                Ok(_) => println!("Successfully erased config."),
                Err(e) => eprintln!("Could not erase config. Error: {}", e)
            }
            match File::create(nash_dir.join("history"))
            {
                Ok(_) => println!("Successfully erased history."),
                Err(e) => eprintln!("Could not erase history. Error: {}", e)
            }
            match File::create(nash_dir.join("alias"))
            {
                Ok(_) => println!("Successfully erased aliases."),
                Err(e) => eprintln!("Could not erase aliases. Error: {}", e)
            }
        }
        Some(v) =>
        {
            if v.parse::<bool>().unwrap_or(false) {
                let nash_dir_disp = nash_dir.display().to_string();
                match remove_file("/usr/bin/nash") {
                    Ok(_) => println!("Successfully deleted /usr/bin/nash."),
                    Err(e) => eprintln!("Could not delete /usr/bin/nash file. Error: {}", e)
                }
                match fs::remove_dir_all(&nash_dir) {
                    Ok(_) => println!("Successfully deleted {}.", nash_dir_disp),
                    Err(e) => eprintln!("Could not delete {} directory. Error: {}", nash_dir_disp, e)
                }
            }            
        }
    }

    exit(1000);
}

fn split_command(cmd: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;

    for c in cmd.trim().chars() {
        match c {
            '"' if !escaped => {
                in_quotes = !in_quotes;
                if !in_quotes && !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
            }
            ' ' if !in_quotes && !escaped => {
                if !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
            }
            '\\' if !escaped => {
                escaped = true;
            }
            _ => {
                if escaped {
                    escaped = false;
                }
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        result.push(current);
    }

    result
}

fn set_conf_rule(conf: &mut Config, cmd: &Vec<String>) -> String {
    if cmd.len() < 2 {
        return "Usage: set <flag> OR set <option> <value>".to_owned();
    }
    let set: bool;

    match cmd.len() {
        2 => {
            // Command is in "set <flag>" format
            let flag: &str = &cmd[1].trim_start_matches('-');
            match flag
            {
                "e" => conf.set_rule("error", "true", true),
                "d" => conf.set_rule("delete_on_reset", "true", true),
                _ => conf.set_rule(flag, "true", false)
            }
            set = true;
        }
        3 => {
            // Command is in "set <option> <value>" format
            let option: &str = &cmd[1];
            let value: &String = &cmd[2];
            match option
            {
                "error" => conf.set_rule("error", value, true),
                "delete_on_reset" => conf.set_rule("delete_on_reset", value, true),
                _ => conf.set_rule(&option, &value, false)
            }
            set = true;
        }
        4 => {
            // Command is in "set <option> <value>" format
            let option: &str = &cmd[1];
            let value: &String = &cmd[2];
            let temp: &bool = &cmd[3].parse::<bool>().unwrap_or(true);
            match option
            {
                "error" => conf.set_rule("error", value, *temp),
                "delete_on_reset" => conf.set_rule("delete_on_reset", value, *temp),
                _ => conf.set_rule(&option, &value, *temp)
            }
            set = true;
        }
        _ => {
            // Invalid usage
            return "Invalid usage. Use 'set <flag>' or 'set <option> <value>'.".to_owned()
        }
    }
    if set
    {
        conf.save_rules();
    }
    return if set {"Successfully set option".to_owned()} else {"Failed to set option".to_owned()}
}

fn unset_conf_rule(conf: &mut Config, cmd: &Vec<String>) -> String
{
    let mut errored: bool = false;
    if cmd.len() == 3
    {
        let temp: bool = match cmd[2].parse::<bool>()
        {
            Ok(b) => b,
            Err(_) => {errored = true; false}
        };
        if errored
        {
            return "You must specify whether the rule is in the temporary or consistent rules.".to_owned();
        }
        conf.remove_rule(&cmd[1], temp).unwrap_or(("".to_owned(), "".to_owned())).1
    }
    else {
        "Usage: unset <option> <temp>".to_owned()
    }
}

fn read_conf(conf: &Config, cmd: &Vec<String>) -> String
{
    if cmd.len() == 3
    {
        let temp: bool = match cmd[2].parse::<bool>()
        {
            Ok(b) => b,
            Err(e) => {println!("Could not determine whether searching the temporary list or consistent rules. Assuming consistent. Recieved error: {}", e); false}
        };
        return match conf.get_rule(&cmd[1], temp)
        {
            None => "Rule not set.".to_owned(),
            Some(s) => s.to_owned()
        };
    } else if cmd.len() == 2
    {
        return match conf.get_rule(&cmd[1], false)
        {
            None => format!("Rule not set in consistent, checking temporary.\n{}", match conf.get_rule(&cmd[1], true)
        {
            None => "Rule not set in temporary",
            Some(c) => c
        }).to_owned(),
            Some(s) => s.to_owned()
        };
    }
    else {
        "Usage: rconf <option> [temp(bool)]".to_owned()
    }
}

fn show_help() -> String {
    "cd <directory>: Change the current directory\n\
     ls [directory] [-l] [-a] [-d]: List contents of a directory\n\
     cp [-r|R] [-f] <source> <destination>: Copy files or directories\n\
     mv [-f] <source> <destination>: Move files or directories\n\
     rm [-f] <file>: Remove a file\n\
     mkdir [-p] <directory>: Create a new directory\n\
     history: Display command history\n\
     exit: Exit the shell\n\
     summon [-w] <command>: Open an *external* command in a new terminal window\n\
     alias <identifier>[=original]: Create an alias for a command\n\
     rmalias <identifier>: Remove an alias for a command\n\
     help: Display this help menu\n\
     set <<<option> <value>>/<flag>>: Set a config rule to true or value\n\
     unset <option> <temp(bool)>: Unset a config rule (unimplemented)\n\
     reset: Reset the application, erase if delete_on_reset rule is true\n\
     rconf <option> [temp(bool)]: Read the value of a config rule (unimplemented)".to_owned()
}

fn special_eval(state: &mut ShellState, conf: &mut Config, cmd: String) -> String {
    let mut result: String = String::new();
    let commands: Vec<String> = cmd.split(';').map(|s| s.trim().to_owned()).collect();

    for command in commands {
        if command.contains("|") {
            result = pipe_eval(command);
        } else if command.contains(">") {
            result = out_redir_eval(state, conf, command);
        } else {
            result = eval(state, conf, command);
        }
    }
    result
}

fn pipe_eval(cmd: String) -> String {
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

fn out_redir_eval(state: &mut ShellState, conf: &mut Config, cmd: String) -> String {
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
            let output: String = eval(state, conf, command);
            match file.write_all(output.as_bytes()) {
                Ok(_) => NO_RESULT.to_owned(),
                Err(e) => format!("Failed to write to file: {}", e),
            }
        }
        Err(e) => format!("Failed to open file: {}", e),
    }
}

fn expand(state: &mut ShellState, cmd: &str) -> String {
    expand_dots(state, &expand_env_vars(&expand_home(cmd).to_string()))
}

fn expand_dots(state: &ShellState, cmd: &str) -> String {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let mut result = Vec::new();

    for part in parts {
        let expanded = if part.contains('.') {
            let path = Path::new(part);
            let mut components = Vec::new();

            for component in path.components() {
                match component {
                    std::path::Component::CurDir => {
                        components.push(state.cwd.clone());
                    },
                    std::path::Component::ParentDir => {
                        if !components.is_empty() {
                            components.pop();
                        } else {
                            let mut parent = PathBuf::from(&state.cwd);
                            parent.pop();
                            components.push(parent.to_string_lossy().into_owned());
                        }
                    },
                    _ => components.push(component.as_os_str().to_string_lossy().into_owned()),
                }
            }

            components.join("/")
        } else {
            part.to_string()
        };

        result.push(expanded);
    }

    result.join(" ")
}

fn expand_home(cmd: &str) -> Cow<str> {
    if cmd.contains('~') {
        match dirs::home_dir() {
            Some(home) => {
                let home_str = home.to_string_lossy();
                Cow::Owned(cmd.replace('~', &home_str))
            }
            None => Cow::Borrowed(cmd),
        }
    } else {
        Cow::Borrowed(cmd)
    }
}

fn expand_env_vars(cmd: &str) -> String {
    let mut result: String = String::new();
    let mut in_var: bool = false;
    let mut var_name: String = String::new();

    for c in cmd.chars() {
        if c == '$' {
            in_var = true;
            var_name.clear();
        } else if in_var {
            if c.is_alphanumeric() || c == '_' {
                var_name.push(c);
            } else {
                in_var = false;
                if let Ok(value) = env::var(&var_name) {
                    result.push_str(&value);
                } else {
                    result.push('$');
                    result.push_str(&var_name);
                }
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    if in_var {
        if let Ok(value) = env::var(&var_name) {
            result.push_str(&value);
        } else {
            result.push('$');
            result.push_str(&var_name);
        }
    }

    result
}

fn handle_summon(cmd_parts: &[String]) -> String {
    let (main_args, flag_args) = parse_args(cmd_parts);
    let wait_for_exit: bool = flag_args.contains_key("w");
    
    if main_args.len() < 1 {
        return "Usage: summon [-w] <command>".to_owned();
    }

    let executable: &String = &main_args[0];
    let args: Vec<&String> = main_args.iter().skip(2).collect();

                // List of common terminal emulators
                let terminals: Vec<&str> = vec![
                    "x-terminal-emulator",
                    "gnome-terminal",
                    "konsole",
                    "xterm",
                    "urxvt",
                    "alacritty",
                    "warp",
                    "termux",
                    "qterminal",
                    "kitty",
                    "tilix",
                    "terminator",
                    "rxvt",
                    "st",
                    "terminology",
                    "hyper",
                    "iterm2",
                ];
        
                let mut installed_terminals: Vec<&str> = Vec::new();
        
                // Check for installed terminals
                for &terminal in &terminals {
                    if let Ok(output) = Command::new("which").arg(terminal).output() {
                        if !output.stdout.is_empty() {
                            installed_terminals.push(terminal);
                            println!("Found terminal: {}", terminal);
                        }
                    }
                }
        
                // No terminal :(
                if installed_terminals.is_empty() {
                    eprintln!("Unable to find a suitable terminal emulator");
                    return NO_RESULT.to_owned();
                }
        
                // Use the first installed terminal in the list
                let terminal: &str = &installed_terminals[0];
                println!("Using terminal: {}", terminal);
                

        let result: Result<process::Child, Error> = match terminal {
            "gnome-terminal" => {
                let mut cmd: Vec<&str> = vec!["bash", "-c", executable];
                cmd.extend(args.iter().map(|s| s.as_str()));
                println!("Executing: {} -- {} {:?}", terminal, cmd.join(" "), cmd);
                Command::new(terminal)
                    .args(&["--"])
                    .args(&cmd)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
            },
            "warp" => {
                let mut cmd: Vec<&str> = vec![executable.as_str()];
                cmd.extend(args.iter().map(|s| s.as_str()));
                println!("Executing: {} --cmd {} {:?}", terminal, cmd.join(" "), cmd);
                Command::new(terminal)
                    .arg("--cmd")
                    .args(&cmd)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
            },
            "termux" => {
                let mut cmd: Vec<&str> = vec![executable.as_str()];
                cmd.extend(args.iter().map(|s| s.as_str()));
                println!("Executing: {} -e {} {:?}", terminal, cmd.join(" "), cmd);
                Command::new(terminal)
                    .arg("-e")
                    .args(&cmd)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
            },
            _ => {
                let mut cmd: Vec<&str> = vec![executable.as_str()];
                cmd.extend(args.iter().map(|s| s.as_str()));
                println!("Executing: {} -e {} {:?}", terminal, cmd.join(" "), cmd);
                Command::new(terminal)
                    .arg("-e")
                    .args(&cmd)
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
                        Ok(status) => return format!("Process exited with status: {}", status),
                        Err(e) => return format!("Error waiting for process: {}", e),
                    }
                } else {
                    return child.id().to_string()
                }
            },
            Err(e) => return format!("An error occurred: {} (Command: {})", e, executable),
        }
}

fn env_var_eval(_state: &ShellState, cmd: String) -> String {
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
                return execute_external_command(&value, &[value.clone()]);
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

fn handle_history() -> String {
    let history_file: PathBuf = get_history_file_path();
    match fs::read_to_string(history_file) {
        Ok(contents) => {
            for (i, line) in contents.lines().enumerate() {
                println!("{}: {}", i + 1, line);
            }
            NO_RESULT.to_owned()
        }
        Err(e) => format!("Failed to read history: {}", e),
    }
}

fn handle_alias(cmd_parts: &[String]) -> String {
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
            let command: &str = command[1..].trim().trim_matches('\'').trim_matches('"');
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

fn handle_remove_alias(cmd_parts: &[String]) -> String {
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

fn load_aliases(path: &PathBuf) -> HashMap<String, String> {
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

fn save_aliases(path: &PathBuf, aliases: &HashMap<String, String>) {
    let content: String = aliases
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(path, content).expect("Unable to write alias file");
}

fn handle_cp(cmd_parts: &[String]) -> String {
    let (main_args, flag_args) = parse_args(cmd_parts);
    let recursive: bool = flag_args.contains_key("r") || flag_args.contains_key("R");
    let force: bool = flag_args.contains_key("f");

    if main_args.len() < 2 {
        println!("{}, {}, {:#?}, {:#?}", recursive, force, main_args, flag_args);
        return "Usage: cp [-r|R] [-f] <source> <destination>".to_owned();
    }

    let src: &String = &main_args[0];
    let dst: &String = &main_args[1];

    match copy_item(src, dst, recursive, force) {
        Ok(_) => "Successfully copied item.".to_owned(),
        Err(e) => format!("An error occurred: {}, {:#?}, {:#?}", e, main_args, flag_args),
    }
}

fn copy_item(src: &str, dst: &str, recursive: bool, force: bool) -> io::Result<()> {
    let src_path: &Path = Path::new(src);
    let dst_path: &Path = Path::new(dst);

    if src_path.is_dir() && !recursive {
        return Err(io::Error::new(io::ErrorKind::Other, "Cannot copy directory without -r flag"));
    }

    if src_path.is_dir() {
        copy_dir_all(src_path, dst_path, force)?;
    } else {
        if dst_path.is_dir() {
            let file_name: &OsStr = src_path.file_name().unwrap();
            let dst_file_path: PathBuf = dst_path.join(file_name);
            if force || !dst_file_path.exists() {
                fs::copy(src_path, dst_file_path)?;
            } else {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Destination file already exists"));
            }
        } else {
            if force || !dst_path.exists() {
                fs::copy(src_path, dst_path)?;
            } else {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Destination file already exists"));
            }
        }
    }
    Ok(())
}

fn handle_mv(cmd_parts: &[String]) -> String {
    let (main_args, flag_args) = parse_args(cmd_parts);
    let force: bool = flag_args.contains_key("f");

    if main_args.len() < 2 {
        return "Usage: mv [-f] <source> <destination>".to_owned();
    }

    let src: &String = &main_args[main_args.len() - 2];
    let dst: &String = &main_args[main_args.len() - 1];

    match move_item(src, dst, force) {
        Ok(_) => "Successfully moved item.".to_owned(),
        Err(e) => format!("An error occurred: {}, {}, {}", e, src, dst),
    }
}

fn move_item(src: &str, dst: &str, force: bool) -> io::Result<()> {
    let src_path: &Path = Path::new(src);
    let dst_path: &Path = Path::new(dst);

    if src_path.is_dir() {
        if dst_path.exists() && dst_path.is_dir() {
            let new_dst: PathBuf = dst_path.join(src_path.file_name().unwrap());
            if force || !new_dst.exists() {
                fs::rename(src_path, new_dst)?;
            } else {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Destination directory already exists"));
            }
        } else {
            if force || !dst_path.exists() {
                fs::rename(src_path, dst_path)?;
            } else {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Destination already exists"));
            }
        }
    } else {
        if dst_path.is_dir() {
            let file_name: &OsStr = src_path.file_name().unwrap();
            let dst_file_path: PathBuf = dst_path.join(file_name);
            if force || !dst_file_path.exists() {
                fs::rename(src_path, dst_file_path)?;
            } else {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Destination file already exists"));
            }
        } else {
            if force || !dst_path.exists() {
                fs::rename(src_path, dst_path)?;
            } else {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Destination file already exists"));
            }
        }
    }
    Ok(())
}

fn handle_rm(cmd_parts: &[String]) -> String {
    let (main_args, flag_args) = parse_args(cmd_parts);
    let force: bool = flag_args.contains_key("f");
    let recursive: bool = flag_args.contains_key("r");

    if main_args.len() < 1 {
        return "Usage: rm [-f] [-r] <file_or_directory>".to_owned();
    }

    let path_str: &String = &main_args[0];
    let path: &Path = Path::new(path_str);

    if !path.exists() {
        return format!("File or directory not found: {}", path_str);
    }

    let is_dir = path.is_dir();

    if is_dir && !recursive {
        return format!("Cannot remove '{}': Is a directory. Use -r flag for recursive removal.", path_str);
    }

    if force || confirm_removal(path_str) {
        let result: Result<(), Error> = if is_dir {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        };

        match result {
            Ok(_) => format!("{} removed successfully.", if is_dir { "Directory" } else { "File" }),
            Err(e) => format!("Error removing {}: {}", if is_dir { "directory" } else { "file" }, e),
        }
    } else {
        "Operation cancelled.".to_owned()
    }
}

fn confirm_removal(path: &str) -> bool {
    println!("Are you sure you want to remove {}? (y/N)", path);
    let mut input: String = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_lowercase() == "y"
}

fn handle_mkdir(cmd_parts: &[String]) -> String {
    let (main_args, flag_args) = parse_args(cmd_parts);
    let parents: bool = flag_args.contains_key("p");

    if main_args.len() < 1 {
        return "Usage: mkdir [-p] <directory_path>".to_owned();
    }

    let dir_path: &String = &main_args[main_args.len() - 1];

    if parents {
        match fs::create_dir_all(dir_path) {
            Ok(_) => "Successfully created directory and any necessary parent directories.".to_owned(),
            Err(e) => format!("An error occurred: {}", e),
        }
    } else {
        match fs::create_dir(dir_path) {
            Ok(_) => "Successfully created directory.".to_owned(),
            Err(e) => format!("An error occurred: {}", e),
        }
    }
}

fn handle_ls(state: &ShellState, cmd_parts: &[String]) -> String {
    let (main_args, flag_args) = parse_args(cmd_parts);
    let long_format: bool = flag_args.contains_key("l");
    let show_hidden: bool = flag_args.contains_key("a");
    let list_dir_itself: bool = flag_args.contains_key("d");

    let path: PathBuf = if main_args.len() > 0 {
        if main_args[0].starts_with('/') {
            PathBuf::from(&main_args[0])
        } else {
            PathBuf::from(&state.cwd).join(&main_args[0])
        }
    } else {
        PathBuf::from(&state.cwd)
    };

    if list_dir_itself {
        return list_directory_entry(&path, long_format);
    }

    list_directory(&path, long_format, show_hidden)
}

fn list_directory(path: &Path, long_format: bool, show_hidden: bool) -> String {
    let mut out: String = String::new();
    if path.is_file()
    {
        let md: fs::Metadata = fs::metadata(path).unwrap_or(fs::metadata("/etc/fstab").unwrap());
        return format!("{}", color_filetype(md.file_type(), &path.to_string_lossy()));
    }

    match fs::read_dir(path) {
        Ok(entries) => {
            let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
            entries.sort_by_key(|e: &fs::DirEntry| e.file_name());

            for entry in entries {
                let file_name: std::ffi::OsString = entry.file_name();
                let file_name_str: Cow<'_, str> = file_name.to_string_lossy();

                if !show_hidden && file_name_str.starts_with('.') {
                    continue;
                }

                if long_format {
                    let entry_path: PathBuf = entry.path();
                    if let Ok(metadata) = entry.metadata() {
                        out.push_str(&format_long_listing(&entry_path, &metadata));
                    } else {
                        eprintln!("Failed to get metadata for {:?}", entry_path);
                    }
                } else {
                    let file_t: fs::FileType = entry.file_type().unwrap();
                    
                    let styled_output: console::StyledObject<&_> = color_filetype(file_t, &file_name_str);
                    
                    out.push_str(&format!("{} ", styled_output));
                }
            }
            out = out.trim().to_owned();
            if out.is_empty() {
                "Directory is empty".to_owned()
            } else {
                out
            }
        }
        Err(e) => {
            format!("Failed to read directory: {} ({})", path.display(), e)
        }
    }
}

fn color_filetype<'a>(file_t: fs::FileType, file_name_str: &'a Cow<'a, str>) ->  console::StyledObject<&'a Cow<'a, str>>
{
    return if file_t.is_dir() {
        Style::new().fg(Color::Blue).bold().apply_to(&file_name_str)
    } else if file_t.is_file() {
        let extension: &str = file_name_str.split('.').last().unwrap_or("");
        match extension {
            "sh" | "bash" | "zsh" | "fish" => Style::new().fg(Color::Green).apply_to(&file_name_str),
            "tar" | "tgz" | "gz" | "zip" | "rar" | "7z" => Style::new().fg(Color::Red).apply_to(&file_name_str),
            "jpg" | "jpeg" | "gif" | "png" | "bmp" => Style::new().fg(Color::Magenta).apply_to(&file_name_str),
            "mp3" | "wav" | "flac" => Style::new().fg(Color::Cyan).apply_to(&file_name_str),
            "pdf" | "epub" | "mobi" => Style::new().fg(Color::Yellow).apply_to(&file_name_str),
            "exe" | "dll" => Style::new().fg(Color::Green).bold().apply_to(&file_name_str),
            _ => Style::new().apply_to(&file_name_str),
        }
    } else if file_t.is_symlink() {
        Style::new().fg(Color::Cyan).apply_to(&file_name_str)
    } else {
        Style::new().apply_to(&file_name_str)
    };
}

fn list_directory_entry(path: &Path, long_format: bool) -> String {
    if long_format {
        let metadata = fs::metadata(path).unwrap();
        format_long_listing(path, &metadata)
    } else {
        let styled_output = style_path(path);
        format!("{}\n", styled_output)
    }
}

fn format_long_listing(path: &Path, metadata: &fs::Metadata) -> String {
    let file_type: &str = get_file_type(metadata);
    let permissions: String = format_permissions(metadata.mode());
    let links: u64 = metadata.nlink();
    let owner: String = get_owner(metadata.uid());
    let group: String = get_group(metadata.gid());
    let size: u64 = metadata.len();
    let modified: SystemTime = metadata.modified().unwrap();
    let modified_str: String = format_time(modified);
    let name: Cow<'_, str> = path.file_name().unwrap_or_default().to_string_lossy();
    let styled_name: console::StyledObject<String> = style_path(path);

    let symlink_target: String = if metadata.file_type().is_symlink() {
        fs::read_link(path)
            .map(|target| format!(" -> {}", target.to_string_lossy()))
            .unwrap_or_else(|_| String::new())
    } else {
        String::new()
    };

    format!(
        "{}{} {:>4} {:>8} {:>8} {:>8} {} {}{}\n",
        file_type,
        permissions,
        links,
        owner,
        group,
        size,
        modified_str,
        styled_name,
        symlink_target
    )
}

fn get_file_type(metadata: &fs::Metadata) -> &'static str {
    if metadata.is_dir() {
        "d"
    } else if metadata.file_type().is_symlink() {
        "l"
    } else {
        "-"
    }
}

fn format_permissions(mode: u32) -> String {
    let user = format_permission_triple(mode >> 6);
    let group = format_permission_triple(mode >> 3);
    let other = format_permission_triple(mode);
    format!("{}{}{}", user, group, other)
}

fn format_permission_triple(mode: u32) -> String {
    let read = if mode & 0b100 != 0 { "r" } else { "-" };
    let write = if mode & 0b010 != 0 { "w" } else { "-" };
    let execute = if mode & 0b001 != 0 { "x" } else { "-" };
    format!("{}{}{}", read, write, execute)
}

fn format_time(time: SystemTime) -> String {
    let datetime: DateTime<Local> = time.into();
    datetime.format("%b %d %H:%M").to_string()
}

fn get_owner(uid: u32) -> String {
    // Get the username
    unsafe {
        let passwd = getpwuid(uid);
        if passwd == ptr::null_mut() {
            return format!("{}", uid);
        }
        
        let username = CStr::from_ptr((*passwd).pw_name);
        username.to_string_lossy().into_owned()
    }
}

fn get_group(gid: u32) -> String {
    // Get the group
    unsafe {
        let group = getgrgid(gid);
        if group == ptr::null_mut() {
            return format!("{}", gid);
        }
        
        let groupname = CStr::from_ptr((*group).gr_name);
        groupname.to_string_lossy().into_owned()
    }
}


fn style_path(path: &Path) -> console::StyledObject<String> {
    let name: Cow<'_, str> = path.file_name().unwrap_or_default().to_string_lossy();
    let metadata: fs::Metadata = fs::metadata(path).unwrap();
    if metadata.file_type().is_symlink() {
        Style::new().fg(Color::Black).apply_to(name.to_string())
    } else if metadata.is_dir() {
        Style::new().fg(Color::Blue).bold().apply_to(name.to_string())
    } else if metadata.permissions().mode() & 0o111 != 0 {
        Style::new().fg(Color::Green).apply_to(name.to_string())
    } else {
        let extension = name.split('.').last().unwrap_or("");
        match extension {
            "tar" | "tgz" | "gz" | "zip" | "rar" | "7z" => Style::new().fg(Color::Red).apply_to(name.to_string()),
            "jpg" | "jpeg" | "gif" | "png" | "bmp" => Style::new().fg(Color::Magenta).apply_to(name.to_string()),
            "mp3" | "wav" | "flac" => Style::new().fg(Color::Cyan).apply_to(name.to_string()),
            "pdf" | "epub" | "mobi" => Style::new().fg(Color::Yellow).apply_to(name.to_string()),
            _ => Style::new().apply_to(name.to_string()),
        }
    }
}

fn handle_cd(state: &mut ShellState, cmd_parts: &[String]) -> String {
    match cmd_parts.len() {
        1 => {
            "No directory passed. Usage: cd <directory>".to_owned()
        }
        2 => {
            let new_path: PathBuf = if cmd_parts[1].starts_with('/') {
                PathBuf::from(&cmd_parts[1])
            } else {
                PathBuf::from(&state.cwd).join(&cmd_parts[1])
            };

            if new_path.is_dir() {
                // Canonicalize the path to resolve any ".." or "." components
                match new_path.canonicalize() {
                    Ok(canonical_path) => {
                        state.cwd = canonical_path.to_string_lossy().into_owned();
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


// TODO: use parse_args() in handling nash's arguments.
async fn handle_nash_args(conf: &mut Config, args: Vec<String>) {
    // Check if arg 1 is a path, if so, run it as a series of commands (like bash's .sh running impl) (scripting)
    // PLACEHOLDER, WILL NOT WORK LIKE INTENDED!!
    if args.len() > 1 && Path::new(&args[1]).exists() {
        let script_path: &String = &args[1];
        match fs::read_to_string(script_path) {
            Ok(contents) => {
                let mut state: ShellState = ShellState {
                    cwd: env::current_dir()
                        .unwrap_or_else(|_| PathBuf::from("/"))
                        .to_string_lossy()
                        .to_string(),
                    hostname: fallible::hostname().unwrap(),
                    username: fallible::username().unwrap(),
                };
                for line in contents.lines() {
                    let result: String = eval(&mut state, conf, line.to_string());
                    print(result);
                }
            }
            Err(e) => eprintln!("Failed to read script file: {}", e),
        }
        return;
    }

    let force_update: bool =
        args.contains(&"-f".to_string()) || args.contains(&"--force".to_string());

    // Handle other command-line arguments
    if args.contains(&"--version".to_string()) {
        println!("v0.0.9.6");
        return;
    }

    if args.contains(&"--update".to_string()) {
        if !force_update {
            println!("Checking for updates...");

            // Compare local and remote version
            let local_ver: String = get_local_version();
            let remote_ver: String = get_remote_version().await;

            if local_ver.trim() != remote_ver.trim() {
                println!(
                    "Update detected. Local version: {}, Remote version: {}",
                    local_ver, remote_ver
                );
                update_nash().await;
            } else {
                println!(
                    "Nash is already up to date in major updates (version {}).",
                    local_ver
                );
            }
        } else {
            println!("Updating nash without version check as force flag was used.");
            update_nash().await;
        }
        return;
    }
    // If no recognized arguments, print usage
    print_usage();
}

fn print_usage() {
    println!("Usage: nash [OPTION] [SCRIPT]");
    println!("Options:");
    println!("  --version    Display the current version of Nash");
    println!("  --update     Check for updates and install if available");
    println!("  -f, --force  Force the operation (if used with --update, update even if no new version is detected)");
    println!("  <script>     Run the specified script file (heavily discouraged, unstable)");
}

fn get_local_version() -> String {
    match Command::new("nash").arg("--version").output() {
        Ok(output) => {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
        Err(e) => {
            eprintln!(
                "An error occurred when trying to get the version of nash: {}",
                e
            );
            return format!("Failed to get nash version: {}", e);
        }
    }
}
async fn get_remote_version() -> String {
    let url: &str = "https://raw.githubusercontent.com/barely-a-dev/Nash/refs/heads/main/ver";
    let response: reqwest::Response = reqwest::get(url)
        .await
        .expect("Failed to fetch remote version");
    response
        .text()
        .await
        .expect("Failed to read remote version")
        .trim()
        .to_string()
}

async fn update_nash() {
    // Check if git is installed
    println!("Checking if Git is installed...");
    match Command::new("git").arg("--version").output() {
        Ok(_) => println!("Git is installed."),
        Err(_) => {
            println!("Git is not installed. Please install Git and try again.");
            return;
        }
    }

    // Check if Rust is installed
    println!("Checking if Rust is installed...");
    match Command::new("rustc").arg("--version").output() {
        Ok(_) => println!("Rust is installed."),
        Err(_) => {
            println!("Rust is not installed. Please install Rust (https://www.rust-lang.org/tools/install) and try again.");
            return;
        }
    }

    // Clone or pull the repository
    let repo_url: &str = "https://github.com/barely-a-dev/Nash.git";
    let temp_dir: PathBuf = env::temp_dir().join("nash_update");
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).expect("Failed to remove existing temp directory");
    }
    match Repository::clone(repo_url, &temp_dir) {
        Ok(_) => println!("Repository cloned successfully"),
        Err(e) => {
            println!("Failed to clone: {}", e);
            return;
        }
    }

    // Navigate to the cloned directory
    env::set_current_dir(&temp_dir).expect("Failed to change directory");

    // Build the project
    println!("Building the project...");
    match Command::new("cargo").args(&["build", "--release"]).status() {
        Ok(status) if status.success() => println!("Project built successfully."),
        _ => {
            println!("Failed to build the project");
            return;
        }
    }

    // Copy the binary to /usr/bin/nash (doesn't work.) Best idea to fix requires "summon cp" (an internal command while summon only supports external commands. Summoning internal commands requires a -c nash flag that takes a command and runs it. (So summon cp will do summon nash -c cp))
    println!("Copying the binary to /usr/local/bin/nash...");
    println!("When the window pops up, enter your password and press enter.");
    match Command::new("echo").args(&["cp", "target/release/nash", "/usr/bin/nash", ">> copy.sh"]).status() {
        Ok(status) if status.success() => println!("Copy script created successfully."),
        _ => {
            println!("Failed to create the copy script.");
            return;
        }
    }

    println!("{}", handle_summon(&["-w".to_owned(), format!("\'cd /tmp/{} && sudo chmod +x ./copy.sh && sudo ./copy.sh\'", temp_dir.to_string_lossy().to_string())]));

    // Clean up
    fs::remove_dir_all(&temp_dir).expect("Failed to remove temp directory");

    println!("Update completed successfully!");
}


fn execute_external_command(cmd: &str, cmd_parts: &[String]) -> String {
    match find_command_in_path(cmd) {
        Some(path) => {
            let mut command: Command = Command::new(path);
            if cmd_parts.len() > 1 {
                command.args(&cmd_parts[1..]);
            }

            command.stdin(Stdio::inherit());
            command.stdout(Stdio::inherit());
            command.stderr(Stdio::inherit());

            match command.status() {
                Ok(status) => {
                    if status.success() {
                        NO_RESULT.to_owned()
                    } else {
                        format!("Command exited with status: {}", status)
                    }
                }
                Err(e) => format!("Failed to execute command: {}", e),
            }
        }
        None => format!("Command not found: {}", cmd),
    }
}


fn execute_file(state: &ShellState, path: &str, args: &[String]) -> String {
    let full_path: PathBuf = if path.starts_with('/') {
        PathBuf::from(path)
    } else {
        PathBuf::from(&state.cwd).join(path)
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

fn print(result: String) {
    if !result.is_empty() {
        println!("{}", result);
    }
}

fn copy_dir_all(src: &Path, dst: &Path, force: bool) -> io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry: fs::DirEntry = entry?;
        let ty: fs::FileType = entry.file_type()?;
        let src_path: PathBuf = entry.path();
        let dst_path: PathBuf = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_all(&src_path, &dst_path, force)?;
        } else {
            if force || !dst_path.exists() {
                fs::copy(&src_path, &dst_path)?;
            } else {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Destination file already exists"));
            }
        }
    }
    Ok(())
}
