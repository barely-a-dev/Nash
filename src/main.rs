// MAJOR TODOs: export for env vars; wildcards/regex (*, ?, []); prompt customization with PS1, PS2, etc.
// HUGE TODOs: Scripting (if, elif, else, fi, for, while, funcs, variables); [[ expression ]] and (( expression ))
// TODO: Quotes and escaping?; $(command)/command substitution; process substitution; -c for commands; file descriptor stuff; pushd/popd/dirs

// Current TODO focus: prompt customization
// Most recent update: export/ normal environment variable assignment differentiation
pub mod editing;
pub mod config;
pub mod arguments;
pub mod evaluation;
pub mod globals;
pub mod commands;
pub mod helpers;
pub mod command_parsing;
pub mod jobs;
pub mod script;

#[cfg(feature = "use-libc")]
extern crate libc;

use crate::editing::{CommandHinter, AutoCompleter, LineHighlighter};
use crate::config::Config;
use crate::evaluation::eval;
use crate::globals::ShellState;
use crate::helpers::{get_history_file_path, read_prompt_from_file};
use arguments::parse_arg_vec;
use dirs::home_dir;
use globals::get_nash_dir;
use crate::jobs::{JobControl, RECEIVED_SIGTSTP, setup_signal_handlers};
use crate::script::ScriptExecutor;
use rustyline::{
    completion::{Completer, Pair},
    error::ReadlineError,
    highlight::Highlighter,
    hint::Hinter,
    validate::{MatchingBracketValidator, ValidationContext, ValidationResult, Validator},
    Context, Editor,
};
use rustyline_derive::Helper;
use std::collections::HashMap;
use std::{
    borrow::Cow,
    env,
    fs::File,
    io::{BufReader, BufRead, Write},
    path::{Path, PathBuf},
    process::{exit, Command},
    sync::atomic::Ordering
};
use tokio::runtime::Runtime;
use whoami::fallible;

fn main() {
    env::set_var("0", "nash"); // In case something goes wrong.
    let runtime: Runtime = Runtime::new().unwrap();
    let mut conf: Config = match Config::new()
    {
        Ok(c) => c,
        Err(_) => {eprintln!("An error occurred when initializing the config."); exit(1)}
    };
    let config_file: PathBuf = PathBuf::from(format!("{}/.nash/config", home_dir().unwrap().to_str().unwrap()));
    if File::open(config_file).unwrap().metadata().unwrap().len() < 1
    {
        conf.set_rule("hist_size", "500", false);
        conf.save_rules();
    }
    let mut state: ShellState = ShellState {
        hostname: fallible::hostname().unwrap(),
        username: fallible::username().unwrap(),
        history_limit: 500,
        ps1_prompt: read_prompt_from_file(),
        local_vars: HashMap::new()
    };
    
    let job_control: &mut JobControl = &mut JobControl::new();
    eval(&mut state, &mut conf, job_control, "SHELL=/usr/bin/nash".to_owned(), true);
    eval(&mut state, &mut conf, job_control, format!("NASH={}", get_nash_dir().display()).to_owned(), true);
    runtime.block_on(async {
        let args: Vec<String> = std::env::args().collect();
        if args.len() <= 1 {
            repl(&mut state, &mut conf, job_control);
        } else {
            handle_nash_args(&mut conf, job_control, args).await;
        }
    });
}

// IK, this should, by name, be in helpers.rs. I just don't want to put it there.
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

fn repl(state: &mut ShellState, conf: &mut Config, job_control: &mut JobControl) {
    let history_file: PathBuf = get_history_file_path();
    let mut rl: Editor<ShellHelper> = Editor::new();

    if rl.load_history(&history_file).is_err() {
        println!("No previous history.");
    }

    let helper: ShellHelper = ShellHelper {
        completer: AutoCompleter::new(PathBuf::from(env::current_dir().unwrap_or(PathBuf::from("/")))),
        highlighter: LineHighlighter::new(),
        hinter: CommandHinter::new(rl.history()),
        validator: MatchingBracketValidator::new(),
    };
    rl.set_helper(Some(helper));

    // Setup signal handlers
    if let Err(e) = setup_signal_handlers() {
        eprintln!("Warning: Failed to setup signal handlers: {}", e);
    }

    loop {
        let prompt: String = format!("[{}@{} {}]> ", state.username, state.hostname, env::current_dir().unwrap_or(PathBuf::from("/")).display());
        
        // Check if we received SIGTSTP
        if RECEIVED_SIGTSTP.load(Ordering::SeqCst) {
            RECEIVED_SIGTSTP.store(false, Ordering::SeqCst);
            
            // If there's a current foreground job, stop it
            if let Some(current_job) = job_control.get_current_job() {
                if let Err(e) = job_control.stop_job(current_job.pid) {
                    eprintln!("Failed to stop job: {}", e);
                }
                continue;
            }
        }

        match rl.readline(&prompt) {
            Ok(line) => {
                if rl.history().len() >= state.history_limit {
                    let temp_file: PathBuf = history_file.with_extension("temp");
                    {
                        let input: File = File::open(&history_file).unwrap();
                        let reader = BufReader::new(input);
                        let mut output: File = File::create(&temp_file).unwrap();
                
                        for (index, line) in reader.lines().enumerate() {
                            if index != 0 {  // Skip the first line
                                writeln!(output, "{}", line.unwrap()).unwrap();
                            }
                        }
                    }
                    std::fs::rename(temp_file, &history_file).unwrap();
                }
                rl.add_history_entry(line.as_str());                      
                rl.save_history(&history_file).unwrap();
                
                // Before evaluating, ensure we're in the foreground
                unsafe {
                    let shell_pgid = libc::getpgrp();
                    if libc::tcsetpgrp(libc::STDIN_FILENO, shell_pgid) == -1 {
                        eprintln!("Warning: Failed to take terminal control");
                    }
                }
                //println!("main Made it -2 (call eval)");
                let result: String = eval(state, conf, job_control, line, false);
                print(result);
                //println!("main Made it -1 (printed result)");
                
                //println!("main Made it -0.5 (reached if)");
                if RECEIVED_SIGTSTP.load(Ordering::SeqCst) {
                    //println!("main Made it 0");
                    RECEIVED_SIGTSTP.store(false, Ordering::SeqCst);
                    //println!("main Made it 1");
                    println!("\nJob stopped");
                    //println!("main Made it 2");
                    if let Some(job) = job_control.get_current_job() {
                        println!("[{}] Stopped    {}", job.pid, job.command);
                    }
                }
                //println!("main Made it 3 (passed if)");
                job_control.cleanup_jobs();   
                //println!("main Made it 4 (passed cleanup)");             
                
                // Get the history entries before getting the mutable helper
                let history_entries: Vec<String> = rl
                .history()
                .iter()
                .map(|entry| entry.to_string())
                .collect();

                // Now update the helper with the collected history
                if let Some(helper) = rl.helper_mut() {
                helper
                    .completer
                    .update_current_dir(env::current_dir().unwrap_or(PathBuf::from("/")));
                    
                // Update the hinter with our collected history
                helper.hinter.update_history(&history_entries);
                }                                               
            }
            Err(ReadlineError::Interrupted) => {
                // TODO: actually handle as SIGINT
                println!("^C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("^D");
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

async fn handle_nash_args(conf: &mut Config, job_control: &mut JobControl, args: Vec<String>) {
    let (main_args, flag_args) = parse_arg_vec(&args);
    let update: bool = flag_args.contains_key("update") || flag_args.contains_key("u");
    let force: bool = flag_args.contains_key("force") || flag_args.contains_key("f");
    let version: bool = flag_args.contains_key("version") || flag_args.contains_key("v");

    // Check if arg 1 is a path, if so, run it as a series of commands (like bash's .sh running impl) (scripting)
    // PLACEHOLDER, WILL NOT WORK LIKE INTENDED!!
    if main_args.len() > 0 && Path::new(&main_args[0]).exists() {
        let script_path: &Path = Path::new(&main_args[0]);
        let mut state: ShellState = ShellState {
            hostname: fallible::hostname().unwrap(),
            username: fallible::username().unwrap(),
            history_limit: 500,
            ps1_prompt: read_prompt_from_file(),
            local_vars: HashMap::new()
        };
        let mut executor: ScriptExecutor<'_> = ScriptExecutor::new(&mut state, conf, job_control);
        if let Err(e) = executor.execute_script(script_path) {
            eprintln!("Failed to execute script: {}", e);
        }
        return;
    }

    // Handle other command-line arguments
    if version {
        println!("v0.0.9.7.5");
        return;
    }

    if update {
        if Path::new("/usr/bin/nbm").exists()
        {
            if fallible::username().unwrap_or("user".to_string()) == "root"
            {
                if force
                {
                    println!("Update command exited with status: {}", Command::new("nbm").args(["--update", "--force"]).status().unwrap_or(Default::default()).code().unwrap_or(1));
                }
                else
                {
                    println!("Update command exited with status: {}", Command::new("nbm").args(["--update"]).status().unwrap_or(Default::default()).code().unwrap_or(1));
                }
            }
            else
            {
                eprintln!("Nash must be run as root to update.");
            }
        }
        else
        {
            println!("Please install NBM from the GitHub first.");
            return;
        }
        return;
    }

    // If no recognized arguments, print usage
    print_usage();
}

fn print_usage() {
    println!("Usage: nash [OPTION] [SCRIPT]");
    println!("Options:");
    println!("  --version/-v  Display the current version of Nash");
    println!("  --update/-u   Check for updates and install if available");
    println!("  -f, --force   Force the operation (if used with --update, update even if no new version is detected)");
    println!("  <script>      Run the specified script file (heavily discouraged, unstable)");
}

// What was I on when I made this function?? It's so useless...
fn print(result: String) {
    if !result.is_empty() {
        println!("{}", result);
    }
}
