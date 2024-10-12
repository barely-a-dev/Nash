// Important TODOs: Fix updating, which evidently never worked.
// MAJOR TODOs: export for env vars, CONFIG, set/unset for setting config (easy?), quotes and escaping ('', "", \), alias command (easy)
// Absolutely HUGE TODOs: Scripting (if, elif, else, fi, for, while, funcs, variables), wildcards/regex (*, ?, []), ACTUAL ARGUMENTS FOR COMMANDS (like ls -a and rm -f instead of just ls and rm (I am not doing rm -r. It's stupid.))
pub mod editing;

use crate::editing::*;
use dirs;
use git2::Repository;
use rustyline::{
    completion::{Completer, Pair},
    error::ReadlineError,
    highlight::Highlighter,
    hint::Hinter,
    validate::{MatchingBracketValidator, ValidationContext, ValidationResult, Validator},
    Context, Editor,
};
use rustyline_derive::Helper;
use std::{
    borrow::Cow,
    collections::HashMap,
    env,
    ffi::OsStr,
    fs::{self, create_dir, remove_file, OpenOptions},
    io::{self, Error, Write},
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
};
use tokio::runtime::Runtime;
use whoami::fallible;

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
        if args.len() <= 1 {
            let mut state: ShellState = ShellState {
                cwd: std::env::current_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("/"))
                    .to_string_lossy()
                    .to_string(),
                hostname: fallible::hostname().unwrap(),
                username: fallible::username().unwrap(),
            };
            repl(&mut state);
        } else {
            handle_nash_args(args).await;
        }
    });
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

fn repl(state: &mut ShellState) {
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
                let result: String = eval(state, line);
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
}

fn get_prog_dir() -> PathBuf {
    let mut path: PathBuf = dirs::home_dir().expect("Unable to get home directory");
    path.push(".nash");
    fs::create_dir_all(&path).expect("Unable to create .nash directory");
    path
}

fn get_history_file_path() -> PathBuf {
    let mut path: PathBuf = get_prog_dir();
    path.push("history");
    path
}

fn get_alias_file_path() -> PathBuf {
    let mut path: PathBuf = get_prog_dir();
    path.push("alias");
    path
}

fn eval(state: &mut ShellState, cmd: String) -> String {
    let chars_to_check = [';', '|', '>'];

    if cmd.contains(|c| chars_to_check.contains(&c)) {
        return special_eval(state, cmd);
    }

    let expanded_cmd = expand_env_vars(&cmd);
    let cmd_parts: Vec<String> = expanded_cmd
        .trim()
        .split_whitespace()
        .map(String::from)
        .collect();

    if cmd_parts.is_empty() {
        return "Empty command".to_owned();
    }

    // Check if the first part is an environment variable assignment
    if cmd_parts[0].contains('=') {
        return env_var_eval(state, cmd_parts[0].clone());
    }

    // Load aliases
    let alias_file_path = get_alias_file_path();
    let aliases = load_aliases(&alias_file_path);

    // Check for alias and expand if found
    let expanded_cmd_parts = if let Some(alias_cmd) = aliases.get(&cmd_parts[0]) {
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
        "summon" => handle_summon(&expanded_cmd_parts),
        "alias" => handle_alias(&expanded_cmd_parts),
        "rmalias" => handle_remove_alias(&expanded_cmd_parts),
        _ => {
            // If not a built-in command, execute as an external command
            let result = execute_external_command(&expanded_cmd_parts[0], &expanded_cmd_parts);
            if !result.is_empty() {
                println!("{}", result);
            }
            NO_RESULT.to_owned()
        }
    }
}

fn special_eval(state: &mut ShellState, cmd: String) -> String {
    let mut result = String::new();
    let commands: Vec<String> = cmd.split(';').map(|s| s.trim().to_owned()).collect();

    for command in commands {
        if command.contains("|") {
            result = pipe_eval(command);
        } else if command.contains(">") {
            result = out_redir_eval(state, command);
        } else {
            result = eval(state, command);
        }
    }
    result
}

fn pipe_eval(cmd: String) -> String {
    let parts: Vec<String> = cmd.split('|').map(|s| s.trim().to_owned()).collect();

    let mut input = String::new();
    for part in parts {
        let command_parts: Vec<String> = part.split_whitespace().map(String::from).collect();
        let command = &command_parts[0];
        let args = &command_parts[1..];

        // Create a command with the input as stdin
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to start command");

        // Write the previous command's output to this command's input
        if !input.is_empty() {
            let stdin = child.stdin.as_mut().expect("Failed to open stdin");
            stdin.write_all(input.as_bytes()).expect("Failed to write to stdin");
        }

        // Get the output of this command
        let output = child.wait_with_output().expect("Failed to read stdout");

        if output.status.success() {
            input = String::from_utf8_lossy(&output.stdout).into_owned();
        } else {
            return format!("Command failed and output error: {}", String::from_utf8_lossy(&output.stderr));
        }
    }

    input
}

fn out_redir_eval(state: &mut ShellState, cmd: String) -> String {
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

    let command = parts[0].clone();
    let file_path = parts[1].clone();
    let append_mode = cmd.contains(">>");

    let mut file_options = OpenOptions::new();
    file_options.write(true).create(true);
    if append_mode {
        file_options.append(true);
    } else {
        file_options.truncate(true);
    }

    match file_options.open(&file_path) {
        Ok(mut file) => {
            let output = eval(state, command);
            match file.write_all(output.as_bytes()) {
                Ok(_) => NO_RESULT.to_owned(),
                Err(e) => format!("Failed to write to file: {}", e),
            }
        }
        Err(e) => format!("Failed to open file: {}", e),
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
    if cmd_parts.len() >= 2 {
        let executable: &String = &cmd_parts[1];
        let args: Vec<&String> = cmd_parts.iter().skip(2).collect();

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
                let mut cmd = vec!["bash", "-c", executable];
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
                let mut cmd = vec![executable.as_str()];
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
                let mut cmd = vec![executable.as_str()];
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
                let mut cmd = vec![executable.as_str()];
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
            Ok(child) => return child.id().to_string(),
            Err(e) => return format!("An error occurred: {} (Command: {})", e, executable),
        }
    } else {
        "Usage: summon <external command or path to executable file> [args...]".to_owned()
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
    let alias_file_path = get_alias_file_path();
    let mut aliases = load_aliases(&alias_file_path);

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
        let alias_str = cmd_parts[1..].join(" ");
        if let Some(pos) = alias_str.find('=') {
            let (name, command) = alias_str.split_at(pos);
            let name = name.trim();
            let command = command[1..].trim().trim_matches('\'').trim_matches('"');
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

    let alias_name = &cmd_parts[1];
    let alias_file_path = get_alias_file_path();
    let mut aliases = load_aliases(&alias_file_path);

    if aliases.remove(alias_name).is_some() {
        save_aliases(&alias_file_path, &aliases);
        format!("Alias '{}' removed.", alias_name)
    } else {
        format!("Alias '{}' not found.", alias_name)
    }
}

fn load_aliases(path: &PathBuf) -> HashMap<String, String> {
    let mut aliases = HashMap::new();
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
    match cmd_parts.len() {
        3 => match copy_item(&cmd_parts[1], &cmd_parts[2]) {
            Ok(_) => "Successfully copied item.".to_owned(),
            Err(e) => format!("An error occurred: {}", e),
        },
        0..=2 => "Incorrect number of arguments. Usage: cp <source_path> <dest_path> [arguments]"
            .to_owned(),
        _ => "Copy with arguments not implemented yet.".to_owned(),
    }
}

fn handle_mv(cmd_parts: &[String]) -> String {
    match cmd_parts.len() {
        3 => match move_item(&cmd_parts[1], &cmd_parts[2]) {
            Ok(_) => "Successfully moved item.".to_owned(),
            Err(e) => format!("An error occurred: {}", e),
        },
        0..=2 => "Incorrect number of arguments. Usage: mv <source_path> <dest_path>".to_owned(),
        _ => "Args not implemented yet.".to_owned(),
    }
}

fn handle_rm(cmd_parts: &[String]) -> String {
    match cmd_parts.len() {
        2 => match remove_file(&cmd_parts[1]) {
            Ok(_) => "Successfully removed file.".to_owned(),
            Err(e) => format!("An error occurred: {}", e),
        },
        _ => "Incorrect number of arguments. Usage: rm <file_path>".to_owned(),
    }
}

fn handle_mkdir(cmd_parts: &[String]) -> String {
    match cmd_parts.len() {
        2 => match create_dir(&cmd_parts[1]) {
            Ok(_) => "Successfully created directory.".to_owned(),
            Err(e) => format!("An error occurred: {}", e),
        },
        _ => "Incorrect number of arguments. Usage: mkdir <directory_path>".to_owned(),
    }
}

fn handle_ls(state: &ShellState, cmd_parts: &[String]) -> String {
    match cmd_parts.len() {
        1 => list_directory(&state.cwd),
        2 => {
            let path: PathBuf = if cmd_parts[1].starts_with('/') {
                PathBuf::from(&cmd_parts[1])
            } else {
                PathBuf::from(&state.cwd).join(&cmd_parts[1])
            };
            list_directory(path.to_str().unwrap_or(""))
        }
        _ => "Incorrect number of arguments. Usage: ls [directory_path]".to_owned(),
    }
}

fn handle_cd(state: &mut ShellState, cmd_parts: &[String]) -> String {
    match cmd_parts.len() {
        1 => {
            // Change to home directory when no argument is provided
            if let Some(home_dir) = dirs::home_dir() {
                state.cwd = home_dir.to_string_lossy().into_owned();
                NO_RESULT.to_owned()
            } else {
                "Unable to determine home directory".to_owned()
            }
        }
        2 => {
            let new_path: PathBuf = if cmd_parts[1] == ".." {
                PathBuf::from(&state.cwd)
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("/"))
            } else if cmd_parts[1].starts_with('/') {
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

async fn handle_nash_args(args: Vec<String>) {
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
                    let result: String = eval(&mut state, line.to_string());
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
        println!("v0.0.9.5");
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
    let response = reqwest::get(url)
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

    // Copy the binary to /usr/bin/nash (doesn't work.)
    println!("Copying the binary to /usr/local/bin/nash... (If there is an error, run the following command: \"sudo cp /tmp/nash_update/target/release/nash /usr/bin/nash && sudo rm -rf /tmp/nash_update\" to manually finish the update.)");
    match Command::new("sudo").args(&["cp", "target/release/nash", "/usr/bin/nash"]).status() {
        Ok(status) if status.success() => println!("Binary copied successfully."),
        _ => {
            println!("Failed to copy the binary to /usr/bin/nash");
            return;
        }
    }

    // Clean up
    fs::remove_dir_all(&temp_dir).expect("Failed to remove temp directory");

    println!("Update completed successfully!");
}


fn execute_external_command(cmd: &str, cmd_parts: &[String]) -> String {
    match find_command_in_path(cmd) {
        Some(path) => {
            let mut command = Command::new(path);
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
            let full_path = format!("{}/{}", dir, cmd);
            if std::fs::metadata(&full_path).is_ok() {
                return Some(full_path);
            }
        }
    }
    None
}

fn list_directory(path: &str) -> String {
    let mut out: String = String::new();
    let dir_path: &Path = Path::new(path);
    if dir_path.is_file() {
        let file_name: &OsStr = dir_path
            .file_name()
            .expect("Could not get file name of file passed to ls.");
        let fn_str: &str = file_name
            .to_str()
            .expect("Could not convert OsStr to &str.");
        out.push_str(&fn_str);
        return out;
    }

    match fs::read_dir(dir_path) {
        Ok(entries) => {
            for entry in entries {
                if let Ok(entry) = entry {
                    let file_name: std::ffi::OsString = entry.file_name();
                    let file_name_str: std::borrow::Cow<'_, str> = file_name.to_string_lossy();
                    if file_name_str != "." && file_name_str != ".." {
                        // Only display the file/directory name, not the full path
                        out.push_str(&format!("{}\n", file_name_str));
                    }
                }
            }
            if out.is_empty() {
                "Directory is empty".to_owned()
            } else {
                out
            }
        }
        Err(e) => {
            format!("Failed to read directory: {} ({})", path, e)
        }
    }
}

fn print(result: String) {
    if !result.is_empty() {
        println!("{}", result);
    }
}

fn copy_item(src: &str, dst: &str) -> io::Result<()> {
    let src_path: &Path = Path::new(src);
    let dst_path: &Path = Path::new(dst);

    if src_path.is_dir() {
        copy_dir_all(src_path, dst_path)?;
    } else {
        if dst_path.is_dir() {
            let file_name: &OsStr = src_path.file_name().unwrap();
            let dst_file_path: PathBuf = dst_path.join(file_name);
            fs::copy(src_path, dst_file_path)?;
        } else {
            fs::copy(src_path, dst_path)?;
        }
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry: fs::DirEntry = entry?;
        let ty: fs::FileType = entry.file_type()?;
        let src_path: PathBuf = entry.path();
        let dst_path: PathBuf = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn move_item(src: &str, dst: &str) -> io::Result<()> {
    let src_path: &Path = Path::new(src);
    let dst_path: &Path = Path::new(dst);

    if src_path.is_dir() {
        if dst_path.exists() && dst_path.is_dir() {
            let new_dst: PathBuf = dst_path.join(src_path.file_name().unwrap());
            fs::rename(src_path, new_dst)?;
        } else {
            fs::rename(src_path, dst_path)?;
        }
    } else {
        if dst_path.is_dir() {
            let file_name: &OsStr = src_path.file_name().unwrap();
            let dst_file_path: PathBuf = dst_path.join(file_name);
            fs::rename(src_path, dst_file_path)?;
        } else {
            fs::rename(src_path, dst_path)?;
        }
    }
    Ok(())
}
