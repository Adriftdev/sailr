use chrono::Local;
use console::style;
use futures::stream::{Stream, StreamExt}; // Ensure `StreamExt` is in scope for `.boxed()`
use std::io;
use std::pin::Pin; // Import Pin

// Alias for our tagged log format: (PodName, LogContent)
pub type TaggedLog = (String, String);

/// Equivalent to `LogTagger`.
/// Takes a stream of log lines and maps each valid one to a `TaggedLog`.
///
/// The return type is `Pin<Box<dyn ...>>` to make it `Unpin`.
pub fn log_tagger<S>(
    lines_stream: S,
    pod_name: String,
) -> Pin<Box<dyn Stream<Item = TaggedLog> + Send>>
where
    // Add a `Send` bound to the stream, which is good practice for async.
    S: Stream<Item = io::Result<String>> + Send + 'static,
{
    lines_stream
        .filter_map(|line_result| async move { line_result.ok() })
        .map(move |log_line| (pod_name.clone(), log_line))
        .boxed() // This is the magic! It boxes the stream and pins it.
}

/// A struct that holds the state for grouping logs, equivalent to `LogGrouper`.
pub struct LogGrouper {
    prev_pod_name: Option<String>,
}

impl LogGrouper {
    /// Prints a colored header with the pod name and a local timestamp.
    fn print_header(&mut self, pod_name: &str) {
        let now = Local::now();
        let formatted_time = now.format("%Y-%m-%d %H:%M:%S").to_string();

        let header = format!(
            "{} {} {} {} {}",
            style("[").red(),
            style(pod_name).yellow().bold(),
            style("-").red(),
            style(formatted_time).blue(),
            style("]").red()
        );
        println!("{}", header);
    }

    /// Checks if the pod has changed and prints a header if so, then prints the log line.
    pub fn transform_and_print(&mut self, pod_name: &str, log: &str) {
        if self.prev_pod_name.as_deref() != Some(pod_name) {
            self.print_header(pod_name);
            self.prev_pod_name = Some(pod_name.to_string());
        }
        println!("{}", log);
    }
}

/// Factory function to create a new `LogGrouper`.
pub fn log_grouper() -> LogGrouper {
    LogGrouper {
        prev_pod_name: None,
    }
}
