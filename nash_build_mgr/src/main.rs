use std::{fs, process::{Command, exit}, env, path::{PathBuf, Path}};
use git2::Repository;
use reqwest::blocking::Client;
use std::time::Duration;
use whoami::fallible;

fn main()
{
    let mut args: Vec<String> = env::args().collect();
    args.remove(0); // Program name
    let force: bool = args.contains(&"--force".to_string()) || args.contains(&"-f".to_string());
    let do_update: bool = args.contains(&"--update".to_string()) || args.contains(&"-u".to_string());
    let mut unrecognized_arguments: Vec<String> = vec![];
    for arg in &args
    {
        if arg != "--force" && arg != "-f" && arg != "--update" && arg != "-u"
        {
            unrecognized_arguments.push(arg.to_owned());
        }
    }
    if unrecognized_arguments.len() > 0
    {
        for un_arg in unrecognized_arguments
        {
            eprintln!("Unrecognized argument: {}", un_arg);
        }
        exit(1);
    }
    if args.len() < 1 || (!force && !do_update)
    {
        println!("You must pass at least one valid argument.");
        exit(1);
    } else if do_update
    {
        if match fallible::username() {
            Ok(u) => u,  // Remove the &
            Err(e) => {eprintln!("Could not get username. Received error: {}", e); "user".to_string()}
        } == "root"        
        {
            update(force);
        }
        else
        {
            eprintln!("To update, you must run nash build manager as root.");
        }
    }
}

pub fn update(force: bool)
{
    if force
    {
        update_internal();
    }
    else 
    {
        let remote_version: String = get_remote_version();
        let rem_ver: &str = remote_version.trim();
        let local_version: String = get_local_version();
        let loc_ver: &str = local_version.trim();
        if rem_ver == "FAIL"
        {
            eprintln!("Could not fetch remote version.");
            if loc_ver == "FAIL"
            {
                println!("Updating anyway as local version check failed.");
                update_internal();
            }
            return;
        }
        else if loc_ver != rem_ver
        {
            update_internal();
        }
        else
        {
            eprintln!("No update available and force flag was not specified.");
        }
    }
}

pub fn update_internal()
{
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
    let tmp_dir: PathBuf = env::temp_dir().join("nash_update");
    if tmp_dir.exists() {
        if let Err(e) = fs::remove_dir_all(&tmp_dir) {
            println!("ERROR: Failed to remove existing temporary directory: {}", e);
            return;
        }
    }
    match Repository::clone(repo_url, &tmp_dir) {
        Ok(_) => println!("Repository cloned successfully"),
        Err(e) => {
            println!("Failed to clone: {}", e);
            return;
        }
    }

    // Navigate to the cloned directory
    env::set_current_dir(&tmp_dir).expect("Failed to change directory");

    // Build the project
    println!("Building the project...");
    match Command::new("cargo").args(&["build", "--release"]).status() {
        Ok(status) if status.success() => println!("Project built successfully."),
        _ => {
            println!("Failed to build the project");
            return;
        }
    }

    // Copy the binary to /usr/bin/nash
    println!("Copying the binary to /usr/bin/nash...");
    let source_path: PathBuf = tmp_dir.join("target/release/nash");
    let destination_path: &Path = Path::new("/usr/bin/nash");

    match fs::copy(&source_path, destination_path) {
        Ok(_) => println!("Binary successfully copied to /usr/bin/nash"),
        Err(e) => {
            println!("Failed to copy the binary: {}", e);
            println!("Perhaps you reached this point without running as root?");
            return;
        }
    }

    // Clean up: remove the temporary directory
    println!("Cleaning up...");
    if let Err(e) = fs::remove_dir_all(&tmp_dir) {
        println!("Warning: Failed to remove temporary directory: {}", e);
    }

    println!("Update completed successfully!");
}

pub fn get_local_version() -> String {
    match Command::new("nash").arg("--version").output() {
        Ok(output) => {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
        Err(e) => {
            eprintln!(
                "An error occurred when trying to get the version of nash: {}",
                e
            );
            return "FAIL".to_string();
        }        
    }
}

pub fn get_remote_version() -> String {
    let url: &str = "https://raw.githubusercontent.com/barely-a-dev/Nash/refs/heads/main/ver";
    
    let client: Client = Client::new();
    let response: Result<reqwest::blocking::Response, reqwest::Error> = client
        .get(url)
        .timeout(Duration::from_secs(30))
        .send();

    match response {
        Ok(resp) => {
            match resp.text() {
                Ok(text) => text.trim().to_string(),
                Err(_) => "FAIL".to_string(),
            }
        },
        Err(_) => "FAIL".to_string(),
    }
}