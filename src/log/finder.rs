use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn find_latest_log_file_in_dir(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir).ok()?.filter_map(|entry| entry.ok())
        .filter(|entry| {
            let path = entry.path();
            path.is_file() && path.file_name().and_then(|s| s.to_str())
                .map_or(false, |s| s.starts_with("nwclientLog") && s.ends_with(".txt"))
        })
        .max_by_key(|entry| entry.metadata().ok().and_then(|m| m.modified().ok()))
        .map(|entry| entry.path())
}

pub fn find_latest_log_file() -> Option<PathBuf> {
    if cfg!(windows) {
        // Try OneDrive path first
        let onedrive_path = get_onedrive_logs_path();
        if let Some(log_file) = find_latest_log_file_in_dir(&onedrive_path) {
            return Some(log_file);
        }

        // Try regular Documents path
        let regular_path = get_regular_logs_path();
        if let Some(log_file) = find_latest_log_file_in_dir(&regular_path) {
            return Some(log_file);
        }

        None
    } else {
        // Unix-like systems: check both locations and return the most recent
        let mut candidates = Vec::new();

        // Check ~/.local/share/Neverwinter Nights/logs/
        let local_path = get_unix_logs_path();
        if let Some(log_file) = find_latest_log_file_in_dir(&local_path) {
            candidates.push(log_file);
        }

        // Check ~/Documents/Neverwinter Nights/logs/
        let documents_path = get_unix_documents_logs_path();
        if let Some(log_file) = find_latest_log_file_in_dir(&documents_path) {
            candidates.push(log_file);
        }

        // Return the most recently modified file among all candidates
        candidates.into_iter()
            .max_by_key(|path| {
                path.metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            })
    }
}

pub fn get_onedrive_logs_path() -> PathBuf {
    let mut path = PathBuf::new();
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| {
        std::env::var("USERNAME")
            .map(|username| format!("C:\\Users\\{}", username))
            .unwrap_or_else(|_| "C:\\Users\\Default".to_string())
    });

    path.push(home);
    path.push("OneDrive");
    path.push("Documents");
    path.push("Neverwinter Nights");
    path.push("logs");
    path
}

pub fn get_regular_logs_path() -> PathBuf {
    let mut path = PathBuf::new();
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| {
        std::env::var("USERNAME")
            .map(|username| format!("C:\\Users\\{}", username))
            .unwrap_or_else(|_| "C:\\Users\\Default".to_string())
    });

    path.push(home);
    path.push("Documents");
    path.push("Neverwinter Nights");
    path.push("logs");
    path
}

pub fn get_unix_logs_path() -> PathBuf {
    let mut path = PathBuf::new();
    let home = std::env::var("HOME").unwrap_or_else(|_| {
        std::env::var("USER")
            .map(|user| format!("/home/{}", user))
            .unwrap_or_else(|_| "/home/default".to_string())
    });

    path.push(home);
    path.push(".local");
    path.push("share");
    path.push("Neverwinter Nights");
    path.push("logs");
    path
}

pub fn get_unix_documents_logs_path() -> PathBuf {
    let mut path = PathBuf::new();
    let home = std::env::var("HOME").unwrap_or_else(|_| {
        std::env::var("USER")
            .map(|user| format!("/home/{}", user))
            .unwrap_or_else(|_| "/home/default".to_string())
    });

    path.push(home);
    path.push("Documents");
    path.push("Neverwinter Nights");
    path.push("logs");
    path
}

pub fn cleanup_old_log_files() -> io::Result<usize> {
    let mut cleaned_count = 0;
    let one_day_ago = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() - 86400) // 86400 seconds = 1 day
        .unwrap_or(0);
    
    if cfg!(windows) {
        // Clean both OneDrive and regular paths on Windows
        let paths = vec![get_onedrive_logs_path(), get_regular_logs_path()];
        
        for log_dir in paths {
            if log_dir.exists() {
                let entries = fs::read_dir(&log_dir)?;
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                            if filename.starts_with("nwclientLog") && filename.ends_with(".txt") {
                                if let Ok(metadata) = path.metadata() {
                                    if let Ok(modified) = metadata.modified() {
                                        if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                                            if duration.as_secs() < one_day_ago {
                                                match fs::remove_file(&path) {
                                                    Ok(_) => {
                                                        println!("Deleted old log file: {:?}", path);
                                                        cleaned_count += 1;
                                                    }
                                                    Err(e) => {
                                                        println!("Failed to delete {:?}: {}", path, e);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        // Unix cleanup logic - clean both locations
        let paths = vec![get_unix_logs_path(), get_unix_documents_logs_path()];

        for log_dir in paths {
            if log_dir.exists() {
                let entries = fs::read_dir(&log_dir)?;
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                            if filename.starts_with("nwclientLog") && filename.ends_with(".txt") {
                                if let Ok(metadata) = path.metadata() {
                                    if let Ok(modified) = metadata.modified() {
                                        if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                                            if duration.as_secs() < one_day_ago {
                                                match fs::remove_file(&path) {
                                                    Ok(_) => {
                                                        println!("Deleted old log file: {:?}", path);
                                                        cleaned_count += 1;
                                                    }
                                                    Err(e) => {
                                                        println!("Failed to delete {:?}: {}", path, e);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(cleaned_count)
}