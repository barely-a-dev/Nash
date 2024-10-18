use crate::commands::*;
use std::{fs, process::Command, env, path::PathBuf};
use git2::Repository;

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
            return format!("Failed to get nash version: {}", e);
        }
    }
}
pub async fn get_remote_version() -> String {
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

pub async fn update_nash() {
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

    println!("{}", handle_summon(&["bash".to_owned(), "-c".to_owned(), "\'cd /tmp/{} && sudo chmod +x ./copy.sh && sudo ./copy.sh\'".to_owned()]));
    
    // Clean up
    fs::remove_dir_all(&temp_dir).expect("Failed to remove temp directory");

    println!("Update completed successfully!");
}