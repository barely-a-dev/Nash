// Important TODOs: Fix updating, which evidently never worked.
// MAJOR TODOs: export for env vars, CONFIG, set/unset for setting temp config (easy?), quotes and escaping ('', "", \), wildcards/regex (*, ?, [])
// Absolutely HUGE TODOs: Scripting (if, elif, else, fi, for, while, funcs, variables).
pub mod editing;
pub mod config;
pub mod arguments;
pub mod evaluation;
pub mod globals;
pub mod commands;
pub mod helpers;
pub mod command_parsing;

use crate::editing::*;
use crate::config::*;
use crate::evaluation::*;
use crate::globals::*;
use crate::commands::*;
use crate::helpers::*;
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
    env,
    fs,
    path::{Path, PathBuf},
    process::{exit, Command},
};
use tokio::runtime::Runtime;
use whoami::fallible;

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
        env::set_var("IV", eval(&mut state, &mut conf, "env".to_owned(), true));
        if args.len() <= 1 {
            repl(&mut state, &mut conf);
        } else {
            handle_nash_args(&mut conf, args).await;
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
                let result: String = eval(state, conf, line, false);
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
                    let result: String = eval(&mut state, conf, line.to_string(), false);
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

fn print(result: String) {
    if !result.is_empty() {
        println!("{}", result);
    }
}
