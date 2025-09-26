use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn format_duration(seconds: u64) -> String {
    if seconds >= 60 {
        let minutes = seconds / 60;
        let remaining_seconds = seconds % 60;
        format!("[{}m:{}s]", minutes, remaining_seconds)
    } else {
        format!("[{}s]", seconds)
    }
}

pub fn parse_timestamp(timestamp_str: &str) -> u64 {
    // NWN timestamp format is like "Tue Jul 29 14:10:26"
    // Parse the time components and convert to seconds since start of day
    if let Some(time_part) = timestamp_str.split_whitespace().nth(3) {
        let parts: Vec<&str> = time_part.split(':').collect();
        if parts.len() == 3 {
            if let (Ok(hours), Ok(minutes), Ok(seconds)) = (
                parts[0].parse::<u64>(),
                parts[1].parse::<u64>(),
                parts[2].parse::<u64>()
            ) {
                return hours * 3600 + minutes * 60 + seconds;
            }
        }
    }
    
    // Fallback: use hash if parsing fails
    let mut hasher = DefaultHasher::new();
    timestamp_str.hash(&mut hasher);
    (hasher.finish() % (u32::MAX as u64)) + 1000000
}

pub fn get_current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}