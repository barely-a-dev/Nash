use std::{env, fs, io, os::unix::fs::PermissionsExt, process::{Command, exit}};
use reqwest::blocking::Client;
use serde_json::Value;
use indicatif::{ProgressBar, ProgressStyle};
use whoami::fallible::username;

fn main() {
    // Check if running as root
    if !is_root() {
        // Re-run the program as root
        let args: Vec<String> = env::args().collect();
        let status = Command::new("sudo")
            .args(&args)
            .status()
            .expect("Failed to execute process");

        exit(status.code().unwrap_or(1));
    }

    // Proceed with installation
    if let Some(recent_version) = get_most_recent_version() {
        if download_and_install(&recent_version) {
            println!("Installation completed successfully!");
        } else {
            eprintln!("Installation failed.");
        }
    } else {
        eprintln!("Failed to get the most recent version.");
    }
}

fn is_root() -> bool {
    match username()
    {
        Ok(u) => return u == "root",
        Err(e) => {eprintln!("An error occurred checking if the program was run as root: {}. Assuming not.", e); false}
    }
}

fn get_most_recent_version() -> Option<String> {
    let client = Client::new();
    let url = "https://api.github.com/repos/barely-a-dev/Nash/releases/latest";

    let response = client
        .get(url)
        .header("User-Agent", "Nash-Installer")
        .send()
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    let release: Value = response.json().ok()?;
    release["tag_name"].as_str().map(String::from)
}

fn download_and_install(version: &str) -> bool {
    let client = Client::new();
    let base_url = format!("https://github.com/barely-a-dev/Nash/releases/download/{}", version);

    let pb = ProgressBar::new(100);
    pb.set_style(ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("##-"));

    for binary in &["nash", "nbm"] {
        let url = format!("{}/{}", base_url, binary);
        pb.set_message(format!("Downloading {}", binary));

        let mut response = match client.get(&url).send() {
            Ok(resp) => resp,
            Err(e) => {
                eprintln!("Failed to download {}: {}", binary, e);
                return false;
            }
        };

        let mut file = match fs::File::create(binary) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to create file {}: {}", binary, e);
                return false;
            }
        };

        if io::copy(&mut response, &mut file).is_err() {
            eprintln!("Failed to write to file {}", binary);
            return false;
        }

        pb.inc(50); // Increment progress bar by 50% for each file
    }

    pb.finish_with_message("Download completed");

    // Install binaries
    for binary in &["nash", "nbm"] {
        if fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).is_err() {
            eprintln!("Failed to set permissions for {}", binary);
            return false;
        }
        if fs::rename(binary, format!("/usr/bin/{}", binary)).is_err() {
            eprintln!("Failed to move {} to /usr/bin/", binary);
            return false;
        }
    }

    true
}
