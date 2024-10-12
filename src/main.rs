// MAJOR TODOs: environment variables. (0=nash, echo $0 outs "nash"), command autocompletion, export for env vars, CONFIG, set/unset for setting config (easy?), quotes and escaping ('', "", \), alias command (easy)
// Absolutely HUGE TODOs: Scripting (if, elif, else, fi, for, while, funcs, variables), wildcards/regex (*, ?, []), command line options (-c, etc)/(handle_nash_args), ACTUAL ARGUMENTS FOR COMMANDS (like ls -a and rm -f instead of just ls and rm (I am not doing rm -r. It's stupid.))
pub mod completion;

use crate::completion::AutoCompleter;
use git2::Repository;
use tokio::runtime::Runtime;
use dirs;
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
    env, ffi::OsStr, fs::{self, create_dir, remove_file, OpenOptions}, io::{self, Error, Write}, path::{Path, PathBuf}, process::{self, Command}
};
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
        if args.len() <= 1 {
            let mut state = ShellState {
                cwd: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/")).to_string_lossy().to_string(),
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
struct CompletionHelper {
    completer: AutoCompleter,
    validator: MatchingBracketValidator,
}

impl Completer for CompletionHelper {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Result<(usize, Vec<Pair>), rustyline::error::ReadlineError> {
        self.completer.complete(line, pos, ctx)
    }
}

impl Validator for CompletionHelper {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        self.validator.validate(ctx)
    }
}

impl Hinter for CompletionHelper {
    type Hint = String;

    fn hint(&self, _line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<String> {
        None
    }
}


impl Highlighter for CompletionHelper {}

fn repl(state: &mut ShellState) {
    let history_file: PathBuf = get_history_file_path();
    let helper: CompletionHelper = CompletionHelper {
        completer: AutoCompleter::new(),
        validator: MatchingBracketValidator::new(),
    };
    let mut rl: Editor<CompletionHelper> = Editor::new();
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
                    helper.completer.update_current_dir(PathBuf::from(&state.cwd));
                }
            },
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            },
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            },
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

fn eval(state: &mut ShellState, cmd: String) -> String {
    let chars_to_check: [char; 3] = [';', '|', '>'];
    
    if !cmd.contains(|c: char| chars_to_check.contains(&c))
    {
        let cmd_parts: Vec<String> = cmd.trim().split_whitespace().map(String::from).collect();
        let mut cmd_args: Vec<String> = vec![];
        if cmd_parts.len() > 1 {
            cmd_args = cmd_parts[1..].to_vec();
        }
        match cmd_parts.get(0).map(String::as_str) {
            Some(cmd) if cmd.starts_with('.') => execute_file(state, &cmd[1..], &cmd_args),
            Some("cp") => handle_cp(&cmd_parts),
            Some("mv") => handle_mv(&cmd_parts),
            Some("rm") => handle_rm(&cmd_parts),
            Some("mkdir") => handle_mkdir(&cmd_parts),
            Some("ls") => handle_ls(state, &cmd_parts),
            Some("cd") => handle_cd(state, &cmd_parts),
            Some("history") => handle_history(),
            Some("exit") => {
                println!("Exiting...");
                process::exit(0);
            }
            Some("summon") => {
                if cmd_parts.len() == 2 {
                    let executable: &String = &cmd_parts[1];
                    // List of common terminal emulators
                    let terminals: Vec<&str> = vec![
                        "x-terminal-emulator", "gnome-terminal", "konsole", "xterm", "urxvt", "alacritty",
                        "warp", "termux", "qterminal", "kitty", "tilix", "terminator", "rxvt", "st",
                        "terminology", "hyper", "iterm2"
                    ];
            
                    let mut installed_terminals: Vec<&str> = Vec::new();
            
                    // Check for installed terminals
                    for &terminal in &terminals {
                        if Command::new("which").arg(terminal).output().is_ok() {
                            installed_terminals.push(terminal);
                        }
                    }
            
                    // No terminal :(
                    if installed_terminals.is_empty() {
                        eprintln!("Unable to find a suitable terminal emulator");
                        return NO_RESULT.to_owned();
                    }
            
                    // Use the first installed terminal in the list
                    let terminal: &str = &installed_terminals[0];
                    let result: Result<process::Child, Error> = match terminal {
                        "gnome-terminal" => Command::new(terminal)
                            .args(&["--", "bash", "-c", executable])
                            .spawn(),
                        "warp" => Command::new(terminal)
                            .args(&["--cmd", executable])
                            .spawn(),
                        "termux" => Command::new(terminal)
                            .args(&["-e", executable])
                            .spawn(),
                        _ => Command::new(terminal)
                            .args(&["-e", executable])
                            .spawn(),
                    };
            
                    match result {
                        Ok(child) => return child.id().to_string(),
                        Err(e) => return format!("An error occurred: {}", e),
                    }
                } else {
                    "Usage: summon <external command or path to executable file>".to_owned()
                }
            }
            Some(cmd) => execute_external_command(cmd, &cmd_parts),
            None => "Empty command".to_owned(),
        }
    } else {
        special_eval(state, cmd)
    }
}

fn special_eval(state: &mut ShellState, cmd: String) -> String {
    let mut result: String = String::new();
    let commands: Vec<String> = cmd.split(';').map(|s| s.trim().to_owned()).collect();
    
    for command in commands {
        if command.contains("|") {
            result = pipe_eval(command);
        } else if command.contains(">") {
            result = out_redir_eval(state, command);
        } else {
            result = eval(state, command);
        }
        print(result.clone());
    }
    result
}

fn pipe_eval(cmd: String) -> String {
    let parts: Vec<String> = cmd
        .split('|')
        .map(|s| s.trim().to_owned())
        .collect();
    
    let mut input: String = String::new();
    for part in parts {
        let mut command_parts: Vec<String> = part.split_whitespace().map(String::from).collect();
        if !input.is_empty() {
            command_parts.push(input);
        }
        input = execute_external_command(&command_parts[0], &command_parts);
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
            let output: String = eval(state, command);
            match file.write_all(output.as_bytes()) {
                Ok(_) => NO_RESULT.to_owned(),
                Err(e) => format!("Failed to write to file: {}", e),
            }
        },
        Err(e) => format!("Failed to open file: {}", e),
    }
}

fn handle_history() -> String {
    let history_file: PathBuf = get_history_file_path();
    match fs::read_to_string(history_file) {
        Ok(contents) => {
            for (i, line) in contents.lines().enumerate() {
                println!("{}: {}", i + 1, line);
            }
            NO_RESULT.to_owned()
        },
        Err(e) => format!("Failed to read history: {}", e),
    }
}

fn handle_cp(cmd_parts: &[String]) -> String {
    match cmd_parts.len() {
        3 => {
            match copy_item(&cmd_parts[1], &cmd_parts[2]) {
                Ok(_) => "Successfully copied item.".to_owned(),
                Err(e) => format!("An error occurred: {}", e)
            }
        }
        0..=2 => "Incorrect number of arguments. Usage: cp <source_path> <dest_path> [arguments]".to_owned(),
        _ => "Copy with arguments not implemented yet.".to_owned()
    }
}

fn handle_mv(cmd_parts: &[String]) -> String {
    match cmd_parts.len() {
        3 => {
            match move_item(&cmd_parts[1], &cmd_parts[2]) {
                Ok(_) => "Successfully moved item.".to_owned(),
                Err(e) => format!("An error occurred: {}", e)
            }
        }
        0..=2 => "Incorrect number of arguments. Usage: mv <source_path> <dest_path>".to_owned(),
        _ => "Args not implemented yet.".to_owned()
    }
}

fn handle_rm(cmd_parts: &[String]) -> String {
    match cmd_parts.len() {
        2 => {
            match remove_file(&cmd_parts[1]) {
                Ok(_) => "Successfully removed file.".to_owned(),
                Err(e) => format!("An error occurred: {}", e)
            }
        }
        _ => "Incorrect number of arguments. Usage: rm <file_path>".to_owned()
    }
}

fn handle_mkdir(cmd_parts: &[String]) -> String {
    match cmd_parts.len() {
        2 => {
            match create_dir(&cmd_parts[1]) {
                Ok(_) => "Successfully created directory.".to_owned(),
                Err(e) => format!("An error occurred: {}", e)
            }
        }
        _ => "Incorrect number of arguments. Usage: mkdir <directory_path>".to_owned()
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
        },
        _ => "Incorrect number of arguments. Usage: ls [directory_path]".to_owned()
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
        },
        2 => {
            let new_path: PathBuf = if cmd_parts[1] == ".." {
                PathBuf::from(&state.cwd).parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("/"))
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
                    },
                    Err(e) => format!("Error resolving path: {}", e),
                }
            } else {
                format!("Directory not found: {}", new_path.display())
            }
        },
        _ => "Usage: cd [path]".to_owned()
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
                    cwd: env::current_dir().unwrap_or_else(|_| PathBuf::from("/")).to_string_lossy().to_string(),
                    hostname: fallible::hostname().unwrap(),
                    username: fallible::username().unwrap(),
                };
                for line in contents.lines() {
                    let result: String = eval(&mut state, line.to_string());
                    print(result);
                }
            },
            Err(e) => eprintln!("Failed to read script file: {}", e),
        }
        return;
    }

    let force_update: bool = args.contains(&"-f".to_string()) || args.contains(&"--force".to_string());

    // Handle other command-line arguments
    if args.contains(&"--version".to_string()) {
        println!("v0.0.5");
        return;
    }

    if args.contains(&"--update".to_string()) {
        println!("Checking for updates...");

        // Compare local and remote version
        let local_ver: String = get_local_version();
        let remote_ver: String = get_remote_version().await;

        if local_ver.trim() != remote_ver.trim() {
            println!("Update detected. Local version: {}, Remote version: {}", local_ver, remote_ver);
            update_nash().await;
        } else {
            println!("Nash is already up to date in major updates (version {}).", local_ver);
            if force_update {
                println!("Updating anyway as -f or --force was used");
                update_nash().await;
            }
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
    let output: process::Output = Command::new("nash")
        .arg("--version")
        .output()
        .expect("Failed to execute nash --version");

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

async fn get_remote_version() -> String {
    let url: &str = "https://raw.githubusercontent.com/barely-a-dev/Nash/refs/heads/main/ver";
    let response = reqwest::get(url).await.expect("Failed to fetch remote version");
    response.text().await.expect("Failed to read remote version").trim().to_string()
}

async fn update_nash() {
    // Check if git and Rust are installed
    if !Command::new("git").arg("--version").status().unwrap().success() {
        println!("Git is not installed. Please install Git and try again.");
        return;
    }

    if !Command::new("rustc").arg("--version").status().unwrap().success() {
        println!("Rust is not installed. Please install [Rust](https://www.rust-lang.org/tools/install) and try again.");
        return;
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
    if !Command::new("cargo").args(&["build", "--release"]).status().unwrap().success() {
        println!("Failed to build the project");
        return;
    }

    // Copy the binary to /usr/bin/nash
    if !Command::new("sudo").args(&["cp", "target/release/Nash", "/usr/bin/nash"]).status().unwrap().success() {
        println!("Failed to copy the binary to /usr/bin/nash");
        return;
    }

    // Clean up
    fs::remove_dir_all(&temp_dir).expect("Failed to remove temp directory");

    println!("Update completed successfully!");
}

fn execute_external_command(cmd: &str, cmd_parts: &[String]) -> String {
    if let Some(path) = find_command_in_path(cmd) {
        let output: Result<process::Output, Error> = Command::new(path)
            .args(&cmd_parts[1..])  // Use all arguments
            .output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    String::from_utf8_lossy(&output.stdout).into_owned()
                } else {
                    format!("Command failed: {}", String::from_utf8_lossy(&output.stderr))
                }
            }
            Err(e) => format!("Failed to execute command: {}", e),
        }
    } else {
        format!("Command not found: {}", cmd)
    }
}

fn execute_file(state: &ShellState, path: &str, args: &[String]) -> String {
    let full_path: PathBuf = if path.starts_with('/') {
        PathBuf::from(path)
    } else {
        PathBuf::from(&state.cwd).join(path)
    };

    if full_path.is_file() {
        let output: Result<process::Output, Error> = Command::new(&full_path)
            .args(args)
            .output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    String::from_utf8_lossy(&output.stdout).into_owned()
                } else {
                    format!("Command failed: {}", String::from_utf8_lossy(&output.stderr))
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
    if dir_path.is_file()
    {
        let file_name: &OsStr = dir_path.file_name().expect("Could not get file name of file passed to ls.");
        let fn_str: &str = file_name.to_str().expect("Could not convert OsStr to &str.");
        out.push_str(&fn_str);
        return out
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