
use crate::globals::*;
use console::{Style, Color};
use std::{collections::HashMap, ptr, path::PathBuf, io, time::SystemTime, ffi::{CStr, OsStr}, fs, os::unix::fs::{PermissionsExt, MetadataExt}, borrow::Cow, path::Path};
use libc::{getgrgid, getpwuid};
use chrono::{Local, DateTime};

pub fn get_history_file_path() -> PathBuf {
    let mut path: PathBuf = get_nash_dir();
    path.push("history");
    path
}

pub fn get_alias_file_path() -> PathBuf {
    let mut path: PathBuf = get_nash_dir();
    path.push("alias");
    path
}

pub fn load_aliases(path: &PathBuf) -> HashMap<String, String> {
    let mut aliases: HashMap<String, String> = HashMap::new();
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

pub fn save_aliases(path: &PathBuf, aliases: &HashMap<String, String>) {
    let content: String = aliases
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(path, content).expect("Unable to write alias file");
}

pub fn copy_item(src: &str, dst: &str, recursive: bool, force: bool) -> io::Result<()> {
    let src_path: &Path = Path::new(src);
    let dst_path: &Path = Path::new(dst);

    if src_path.is_dir() && !recursive {
        return Err(io::Error::new(io::ErrorKind::Other, "Cannot copy directory without -r flag"));
    }

    if src_path.is_dir() {
        copy_dir_all(src_path, dst_path, force)?;
    } else {
        if dst_path.is_dir() {
            let file_name: &OsStr = src_path.file_name().unwrap();
            let dst_file_path: PathBuf = dst_path.join(file_name);
            if force || !dst_file_path.exists() {
                fs::copy(src_path, dst_file_path)?;
            } else {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Destination file already exists"));
            }
        } else {
            if force || !dst_path.exists() {
                fs::copy(src_path, dst_path)?;
            } else {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Destination file already exists"));
            }
        }
    }
    Ok(())
}

pub fn move_item(src: &str, dst: &str, force: bool) -> io::Result<()> {
    let src_path: &Path = Path::new(src);
    let dst_path: &Path = Path::new(dst);

    if src_path.is_dir() {
        if dst_path.exists() && dst_path.is_dir() {
            let new_dst: PathBuf = dst_path.join(src_path.file_name().unwrap());
            if force || !new_dst.exists() {
                fs::rename(src_path, new_dst)?;
            } else {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Destination directory already exists"));
            }
        } else {
            if force || !dst_path.exists() {
                fs::rename(src_path, dst_path)?;
            } else {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Destination already exists"));
            }
        }
    } else {
        if dst_path.is_dir() {
            let file_name: &OsStr = src_path.file_name().unwrap();
            let dst_file_path: PathBuf = dst_path.join(file_name);
            if force || !dst_file_path.exists() {
                fs::rename(src_path, dst_file_path)?;
            } else {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Destination file already exists"));
            }
        } else {
            if force || !dst_path.exists() {
                fs::rename(src_path, dst_path)?;
            } else {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Destination file already exists"));
            }
        }
    }
    Ok(())
}

pub fn confirm_removal(path: &str) -> bool {
    println!("Are you sure you want to remove {}? (y/N)", path);
    let mut input: String = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_lowercase() == "y"
}

pub fn list_directory(path: &Path, long_format: bool, show_hidden: bool) -> String {
    let mut out: String = String::new();
    if path.is_file()
    {
        let md: fs::Metadata = fs::metadata(path).unwrap_or(fs::metadata("/etc/fstab").unwrap());
        return format!("{}", color_filetype(md.file_type(), &path.to_string_lossy()));
    }

    match fs::read_dir(path) {
        Ok(entries) => {
            let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
            entries.sort_by_key(|e: &fs::DirEntry| e.file_name());

            for entry in entries {
                let file_name: std::ffi::OsString = entry.file_name();
                let file_name_str: Cow<'_, str> = file_name.to_string_lossy();

                if !show_hidden && file_name_str.starts_with('.') {
                    continue;
                }

                if long_format {
                    let entry_path: PathBuf = entry.path();
                    if let Ok(metadata) = entry.metadata() {
                        out.push_str(&format_long_listing(&entry_path, &metadata));
                    } else {
                        eprintln!("Failed to get metadata for {:?}", entry_path);
                    }
                } else {
                    let file_t: fs::FileType = entry.file_type().unwrap();
                    
                    let styled_output: console::StyledObject<&_> = color_filetype(file_t, &file_name_str);
                    
                    out.push_str(&format!("{} ", styled_output));
                }
            }
            out = out.trim().to_owned();
            if out.is_empty() {
                "Directory is empty".to_owned()
            } else {
                out
            }
        }
        Err(e) => {
            format!("Failed to read directory: {} ({})", path.display(), e)
        }
    }
}

pub fn color_filetype<'a>(file_t: fs::FileType, file_name_str: &'a Cow<'a, str>) ->  console::StyledObject<&'a Cow<'a, str>>
{
    return if file_t.is_dir() {
        Style::new().fg(Color::Blue).bold().apply_to(&file_name_str)
    } else if file_t.is_file() {
        let extension: &str = file_name_str.split('.').last().unwrap_or("");
        match extension {
            "sh" | "bash" | "zsh" | "fish" => Style::new().fg(Color::Green).apply_to(&file_name_str),
            "tar" | "tgz" | "gz" | "zip" | "rar" | "7z" => Style::new().fg(Color::Red).apply_to(&file_name_str),
            "jpg" | "jpeg" | "gif" | "png" | "bmp" => Style::new().fg(Color::Magenta).apply_to(&file_name_str),
            "mp3" | "wav" | "flac" => Style::new().fg(Color::Cyan).apply_to(&file_name_str),
            "pdf" | "epub" | "mobi" => Style::new().fg(Color::Yellow).apply_to(&file_name_str),
            "exe" | "dll" => Style::new().fg(Color::Green).bold().apply_to(&file_name_str),
            _ => Style::new().apply_to(&file_name_str),
        }
    } else if file_t.is_symlink() {
        Style::new().fg(Color::Cyan).apply_to(&file_name_str)
    } else {
        Style::new().apply_to(&file_name_str)
    };
}

pub fn list_directory_entry(path: &Path, long_format: bool) -> String {
    if long_format {
        let metadata = fs::metadata(path).unwrap();
        format_long_listing(path, &metadata)
    } else {
        let styled_output = style_path(path);
        format!("{}\n", styled_output)
    }
}

pub fn format_long_listing(path: &Path, metadata: &fs::Metadata) -> String {
    let file_type: &str = get_file_type(metadata);
    let permissions: String = format_permissions(metadata.mode());
    let links: u64 = metadata.nlink();
    let owner: String = get_owner(metadata.uid());
    let group: String = get_group(metadata.gid());
    let size: u64 = metadata.len();
    let modified: SystemTime = metadata.modified().unwrap();
    let modified_str: String = format_time(modified);
    let styled_name: console::StyledObject<String> = style_path(path);

    let symlink_target: String = if metadata.file_type().is_symlink() {
        fs::read_link(path)
            .map(|target| format!(" -> {}", target.to_string_lossy()))
            .unwrap_or_else(|_| String::new())
    } else {
        String::new()
    };

    format!(
        "{}{} {:>4} {:>8} {:>8} {:>8} {} {}{}\n",
        file_type,
        permissions,
        links,
        owner,
        group,
        size,
        modified_str,
        styled_name,
        symlink_target
    )
}

pub fn get_file_type(metadata: &fs::Metadata) -> &'static str {
    if metadata.is_dir() {
        "d"
    } else if metadata.file_type().is_symlink() {
        "l"
    } else {
        "-"
    }
}

pub fn format_permissions(mode: u32) -> String {
    let user = format_permission_triple(mode >> 6);
    let group = format_permission_triple(mode >> 3);
    let other = format_permission_triple(mode);
    format!("{}{}{}", user, group, other)
}

pub fn format_permission_triple(mode: u32) -> String {
    let read = if mode & 0b100 != 0 { "r" } else { "-" };
    let write = if mode & 0b010 != 0 { "w" } else { "-" };
    let execute = if mode & 0b001 != 0 { "x" } else { "-" };
    format!("{}{}{}", read, write, execute)
}

pub fn format_time(time: SystemTime) -> String {
    let datetime: DateTime<Local> = time.into();
    datetime.format("%b %d %H:%M").to_string()
}

pub fn get_owner(uid: u32) -> String {
    // Get the username
    unsafe {
        let passwd = getpwuid(uid);
        if passwd == ptr::null_mut() {
            return format!("{}", uid);
        }
        
        let username = CStr::from_ptr((*passwd).pw_name);
        username.to_string_lossy().into_owned()
    }
}

pub fn get_group(gid: u32) -> String {
    // Get the group
    unsafe {
        let group = getgrgid(gid);
        if group == ptr::null_mut() {
            return format!("{}", gid);
        }
        
        let groupname = CStr::from_ptr((*group).gr_name);
        groupname.to_string_lossy().into_owned()
    }
}


pub fn style_path(path: &Path) -> console::StyledObject<String> {
    let name: Cow<'_, str> = path.file_name().unwrap_or_default().to_string_lossy();
    let metadata: fs::Metadata = fs::metadata(path).unwrap();
    if metadata.file_type().is_symlink() {
        Style::new().fg(Color::Black).apply_to(name.to_string())
    } else if metadata.is_dir() {
        Style::new().fg(Color::Blue).bold().apply_to(name.to_string())
    } else if metadata.permissions().mode() & 0o111 != 0 {
        Style::new().fg(Color::Green).apply_to(name.to_string())
    } else {
        let extension = name.split('.').last().unwrap_or("");
        match extension {
            "tar" | "tgz" | "gz" | "zip" | "rar" | "7z" => Style::new().fg(Color::Red).apply_to(name.to_string()),
            "jpg" | "jpeg" | "gif" | "png" | "bmp" => Style::new().fg(Color::Magenta).apply_to(name.to_string()),
            "mp3" | "wav" | "flac" => Style::new().fg(Color::Cyan).apply_to(name.to_string()),
            "pdf" | "epub" | "mobi" => Style::new().fg(Color::Yellow).apply_to(name.to_string()),
            _ => Style::new().apply_to(name.to_string()),
        }
    }
}
pub fn copy_dir_all(src: &Path, dst: &Path, force: bool) -> io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry: fs::DirEntry = entry?;
        let ty: fs::FileType = entry.file_type()?;
        let src_path: PathBuf = entry.path();
        let dst_path: PathBuf = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_all(&src_path, &dst_path, force)?;
        } else {
            if force || !dst_path.exists() {
                fs::copy(&src_path, &dst_path)?;
            } else {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Destination file already exists"));
            }
        }
    }
    Ok(())
}
