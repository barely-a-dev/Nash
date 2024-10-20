use std::fs;
use std::path::PathBuf;

pub struct ShellState {
    pub username: String,
    pub hostname: String
}
pub const NO_RESULT: &str = "";

pub fn get_nash_dir() -> PathBuf {
    let mut path: PathBuf = dirs::home_dir().expect("Unable to get home directory");
    path.push(".nash");
    fs::create_dir_all(&path).expect("Unable to create .nash directory");
    path
}