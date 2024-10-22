use std::{env, fs, io::Write, os::unix::fs::PermissionsExt, path::{Path, PathBuf}, process::{exit, Command}, time::Duration};
use git2::Repository;
use reqwest::blocking::Client;
use whoami::fallible;
use serde_json::Value;
use indicatif::{ProgressBar, ProgressStyle};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let force: bool = args.contains(&"--force".to_string()) || args.contains(&"-f".to_string());
    let do_update: bool = args.contains(&"--update".to_string()) || args.contains(&"-u".to_string());
    let list: bool = args.contains(&"--list".to_string());
    let set_ver: Option<usize> = args.iter().position(|x| x == "--setver");

    let unrecognized_arguments: Vec<String> = args.iter()
        .filter(|&arg| !["--force", "-f", "--update", "-u", "--setver", "--list"].contains(&arg.as_str()))
        .filter(|&arg| set_ver.map_or(true, |i| *arg != args[i + 1]))
        .cloned()
        .collect();

    if !unrecognized_arguments.is_empty() {
        for un_arg in unrecognized_arguments {
            eprintln!("Unrecognized argument: {}", un_arg);
        }
        exit(1);
    }

    if args.is_empty() || (!force && !do_update && set_ver.is_none() && !list) {
        println!("You must pass at least one valid argument.");
        exit(1);
    } else if list {
        println!("{}", list_releases().unwrap_or("Failed to list releases".to_string()));
        return;
    } else if let Some(set_ver_index) = set_ver {
        if fallible::username().map(|u: String| u == "root").unwrap_or(false) {
            if set_ver_index >= args.len() - 1 {
                println!("Usage: nbm --setver v<version_number> or nbm --setver recent");
                return;
            } else {
                let version: &String = &args[set_ver_index + 1];
                if version == "recent" {
                    if let Some(recent_version) = get_most_recent_version() {
                        if set_version(&recent_version) {
                            println!("Successfully set version to the most recent: {}", recent_version);
                        } else {
                            eprintln!("Failed to set version to the most recent: {}", recent_version);
                        }
                    } else {
                        eprintln!("Failed to get the most recent version");
                    }
                } else {
                    if set_version(version) {
                        println!("Successfully set version to {}", version);
                    } else {
                        eprintln!("Failed to set version to {}", version);
                    }
                }
            }
        } else {
            eprintln!("To update, you must run nash build manager as root.");
        }
    } else if do_update {
        if fallible::username().map(|u| u == "root").unwrap_or(false) {
            update(force);
        } else {
            eprintln!("To update, you must run nash build manager as root.");
        }
    }
}

fn list_releases() -> Result<String, Box<dyn std::error::Error>> {
    let client: Client = Client::new();
    let url: &str = "https://api.github.com/repos/barely-a-dev/Nash/releases";

    let response = client
        .get(url)
        .header("User-Agent", "Nash-GitHub-Release-Checker")
        .timeout(Duration::from_secs(30))
        .send()?;

    if !response.status().is_success() {
        return Err(format!("GitHub API request failed: {}", response.status()).into());
    }

    let releases: Vec<Value> = response.json()?;

    let release_list: String = releases
        .iter()
        .filter_map(|release| release["tag_name"].as_str())
        .collect::<Vec<_>>()
        .join("\n");

    Ok(release_list)
}

fn find_release(ver: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let client: Client = Client::new();
    let url: &str = "https://api.github.com/repos/barely-a-dev/Nash/releases";

    let response: reqwest::blocking::Response = client
        .get(url)
        .header("User-Agent", "Nash-GitHub-Release-Checker")
        .timeout(Duration::from_secs(30))
        .send()?;

    if !response.status().is_success() {
        return Err(format!("GitHub API request failed: {}", response.status()).into());
    }

    let releases: Vec<Value> = response.json()?;

    Ok(releases.iter().any(|release| {
        release["tag_name"].as_str().map_or(false, |tag| tag == ver)
    }))
}

fn get_most_recent_version() -> Option<String> {
    let client: Client = Client::new();
    let url: &str = "https://api.github.com/repos/barely-a-dev/Nash/releases/latest";

    let response: reqwest::blocking::Response = client
        .get(url)
        .header("User-Agent", "Nash-GitHub-Release-Checker")
        .timeout(Duration::from_secs(30))
        .send()
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    let release: Value = response.json().ok()?;
    release["tag_name"].as_str().map(String::from)
}

fn set_version(version: &str) -> bool {
    match find_release(&version) {
        Ok(true) => {
            let pb = ProgressBar::new(100);
            pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("##-"));

            // Download release's nash and nbm file
            pb.set_message("Downloading release files");
            if let Err(e) = download_release_files(version) {
                pb.finish_with_message("Failed to download release files");
                eprintln!("Failed to download release files: {}", e);
                return false;
            }
            pb.set_position(50);

            // Set permissions and move to /usr/bin/
            pb.set_message("Installing binaries");
            if let Err(e) = install_binaries() {
                pb.finish_with_message("Failed to install binaries");
                eprintln!("Failed to install binaries: {}", e);
                return false;
            }
            pb.finish_with_message("Version set successfully");

            true
        },
        Ok(false) => {
            eprintln!("No such release \"{}\"", version);
            false
        },
        Err(e) => {
            eprintln!("Error checking for release: {}", e);
            false
        }
    }
}

fn download_release_files(version: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client: Client = Client::new();
    let base_url: String = format!("https://github.com/barely-a-dev/Nash/releases/download/{}", version);

    for binary in &["nash", "nbm"] {
        let url: String = format!("{}/{}", base_url, binary);
        let response: reqwest::blocking::Response = client.get(&url).send()?;
        let content = response.bytes()?;

        let mut file: fs::File = fs::File::create(binary)?;
        file.write_all(&content)?;
    }

    Ok(())
}

fn install_binaries() -> Result<(), Box<dyn std::error::Error>> {
    for binary in &["nash", "nbm"] {
        fs::set_permissions(binary, fs::Permissions::from_mode(0o755))?;
        fs::rename(binary, format!("/usr/bin/{}", binary))?;
    }

    Ok(())
}

pub fn update(force: bool) {
    if force {
        update_internal();
    } else {
        let remote_version: String = get_remote_version();
        let local_version: String = get_local_version();
        
        match (&remote_version[..], &local_version[..]) {
            ("FAIL", "FAIL") => {
                println!("Updating anyway as both remote and local version checks failed.");
                update_internal();
            },
            ("FAIL", _) => eprintln!("Could not fetch remote version."),
            (_, "FAIL") => {
                println!("Local version check failed. Updating...");
                update_internal();
            },
            (rem_ver, loc_ver) if rem_ver != loc_ver => {
                println!("Update available. Updating...");
                update_internal();
            },
            _ => eprintln!("No update available and force flag was not specified."),
        }
    }
}

pub fn update_internal() {
    let pb = ProgressBar::new(100);
    pb.set_style(ProgressStyle::default_bar()
    .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
    .unwrap_or_else(|_| ProgressStyle::default_bar())
    .progress_chars("##-"));

    // Check if git is installed
    pb.set_message("Checking if Git is installed");
    match Command::new("git").arg("--version").output() {
        Ok(_) => pb.inc(10),
        Err(_) => {
            pb.finish_with_message("Git is not installed. Please install Git and try again.");
            return;
        }
    }

    // Check if Rust is installed
    pb.set_message("Checking if Rust is installed");
    match Command::new("rustc").arg("--version").output() {
        Ok(_) => pb.inc(10),
        Err(_) => {
            pb.finish_with_message("Rust is not installed. Please install Rust and try again.");
            return;
        }
    }

    // Clone or pull the repository
    pb.set_message("Cloning repository");
    let repo_url: &str = "https://github.com/barely-a-dev/Nash.git";
    let tmp_dir: PathBuf = env::temp_dir().join("nash_update");
    if tmp_dir.exists() {
        if let Err(e) = fs::remove_dir_all(&tmp_dir) {
            pb.finish_with_message("Failed to remove existing temporary directory");
            eprintln!("ERROR: Failed to remove existing temporary directory: {}", e);
            return;
        }
    }
    match Repository::clone(repo_url, &tmp_dir) {
        Ok(_) => pb.inc(20),
        Err(e) => {
            pb.finish_with_message("Failed to clone repository");
            eprintln!("Failed to clone: {}", e);
            return;
        }
    }

    // Navigate to the cloned directory
    env::set_current_dir(&tmp_dir).expect("Failed to change directory");

    // Build the project
    pb.set_message("Building the project");
    match Command::new("cargo").args(&["build", "--release"]).status() {
        Ok(status) if status.success() => pb.inc(30),
        _ => {
            pb.finish_with_message("Failed to build the project");
            return;
        }
    }

    // Copy the binary to /usr/bin/nash
    pb.set_message("Copying the binary to /usr/bin/nash");
    let source_path: PathBuf = tmp_dir.join("target/release/nash");
    let destination_path: &Path = Path::new("/usr/bin/nash");

    match fs::copy(&source_path, destination_path) {
        Ok(_) => pb.inc(20),
        Err(e) => {
            pb.finish_with_message("Failed to copy the binary");
            eprintln!("Failed to copy the binary: {}", e);
            eprintln!("Perhaps you reached this point without running as root?");
            return;
        }
    }

    // Clean up: remove the temporary directory
    pb.set_message("Cleaning up");
    if let Err(e) = fs::remove_dir_all(&tmp_dir) {
        eprintln!("Warning: Failed to remove temporary directory: {}", e);
    }
    pb.inc(10);

    pb.finish_with_message("Update completed successfully!");
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