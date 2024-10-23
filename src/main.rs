// Important TODOs: Fix updating, which evidently never worked
// MAJOR TODOs: export for env vars; wildcards/regex (*, ?, []), job control; prompt customization with PS1, PS2, etc.
// HUGE TODOs: Scripting (if, elif, else, fi, for, while, funcs, variables); [[ expression ]] and (( expression ))
pub mod editing;
pub mod config;
pub mod arguments;
pub mod evaluation;
pub mod globals;
pub mod commands;
pub mod helpers;
pub mod command_parsing;
pub mod jobs;

#[cfg(feature = "use-libc")]
extern crate libc;


use crate::editing::{CommandHinter, AutoCompleter, LineHighlighter};
use crate::config::Config;
use crate::evaluation::eval;
use crate::globals::ShellState;
use crate::helpers::get_history_file_path;
use arguments::parse_arg_vec;
use crate::jobs::{JobControl, RECEIVED_SIGTSTP, setup_signal_handlers};
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
    let mut state: ShellState = ShellState {
        hostname: fallible::hostname().unwrap(),
        username: fallible::username().unwrap(),
    };
    let job_control: &mut JobControl = &mut JobControl::new();
    env::set_var("IV", eval(&mut state, &mut conf, job_control, "SHELL=/usr/bin/nash".to_owned(), true));
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
    let helper: ShellHelper = ShellHelper {
        completer: AutoCompleter::new(PathBuf::from(env::current_dir().unwrap_or(PathBuf::from("/")))),
        highlighter: LineHighlighter::new(),
        hinter: CommandHinter::new(),
        validator: MatchingBracketValidator::new(),
    };
    let mut rl: Editor<ShellHelper> = Editor::new();
    rl.set_helper(Some(helper));

    if rl.load_history(&history_file).is_err() {
        println!("No previous history.");
    }

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
                
                // Update the current directory in the AutoCompleter
                if let Some(helper) = rl.helper_mut() {
                    helper
                        .completer
                        .update_current_dir(env::current_dir().unwrap_or(PathBuf::from("/")));
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Need to actually handle as SIGINT
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

async fn handle_nash_args(conf: &mut Config, job_control: &mut JobControl, args: Vec<String>) {
    let (main_args, flag_args) = parse_arg_vec(&args);
    let update: bool = flag_args.contains_key("update") || flag_args.contains_key("u");
    let force: bool = flag_args.contains_key("force") || flag_args.contains_key("f");
    let version: bool = flag_args.contains_key("version") || flag_args.contains_key("v");

    // Check if arg 1 is a path, if so, run it as a series of commands (like bash's .sh running impl) (scripting)
    // PLACEHOLDER, WILL NOT WORK LIKE INTENDED!!
    if main_args.len() > 0 && Path::new(&main_args[0]).exists() {
        let script_path: &String = &main_args[0];
        match fs::read_to_string(script_path) {
            Ok(contents) => {
                let mut state: ShellState = ShellState {
                    hostname: fallible::hostname().unwrap(),
                    username: fallible::username().unwrap(),
                };
                for line in contents.lines() {
                    let result: String = eval(&mut state, conf, job_control, line.to_string(), false);
                    print(result);
                }
            }
            Err(e) => eprintln!("Failed to read script file: {}", e),
        }
        return;
    }

    // Handle other command-line arguments
    if version {
        println!("v0.0.9.7");
        return;
    }

    if update {
        if Path::new("/usr/bin/nbm").exists()
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
