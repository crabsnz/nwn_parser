#!/bin/bash
# Simple test script to process the test log file and see debug output

cd /home/dan/rust/nwn_parser

# Create a simple Rust test program
cat > test_immunity_main.rs << 'EOF'
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
EOF

echo "Test file created. Checking if we can access process_full_log_file..."
grep -n "pub fn process_full_log_file" src/log/watcher.rs || grep -n "fn process_full_log_file" src/log/watcher.rs