use std::sync::{Arc, Mutex};
use std::fs;

// Include the log watcher module
fn main() {
    // Read test file
    let content = fs::read_to_string("test_immunity_logs.txt").expect("Failed to read test file");

    // Create logs state
    let logs_state: Arc<Mutex<Vec<nwn_parser::gui::logs_window::LogEntry>>> = Arc::new(Mutex::new(Vec::new()));

    // Process the content
    nwn_parser::log::watcher::process_full_log_file_for_test(&content, &logs_state);

    // Print results
    println!("\n=== FINAL LOG ENTRIES ===");
    if let Ok(logs) = logs_state.lock() {
        for entry in logs.iter() {
            println!("{} {}", entry.timestamp, entry.content);
        }
    }
}
