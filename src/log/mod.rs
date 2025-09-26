pub mod finder;
pub mod watcher;

pub use finder::{find_latest_log_file, cleanup_old_log_files};
pub use watcher::{log_watcher_thread, process_full_log_file};