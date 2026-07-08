use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct SailrUI {
    inner: Arc<Mutex<SailrUIInner>>,
    is_tty: bool,
    quiet: Arc<AtomicBool>,
    verbose: Arc<AtomicBool>,
}

struct SailrUIInner {
    multi: MultiProgress,
    spinners: HashMap<String, ProgressBar>,
}

impl SailrUI {
    pub fn new(quiet: bool, verbose: bool) -> Self {
        let is_tty = console::Term::stderr().is_term();
        Self {
            inner: Arc::new(Mutex::new(SailrUIInner {
                multi: MultiProgress::new(),
                spinners: HashMap::new(),
            })),
            is_tty,
            quiet: Arc::new(AtomicBool::new(quiet)),
            verbose: Arc::new(AtomicBool::new(verbose)),
        }
    }

    pub fn set_quiet(&self, val: bool) {
        self.quiet.store(val, Ordering::SeqCst);
    }

    pub fn set_verbose(&self, val: bool) {
        self.verbose.store(val, Ordering::SeqCst);
    }

    pub fn is_verbose(&self) -> bool {
        self.verbose.load(Ordering::SeqCst)
    }

    pub fn is_quiet(&self) -> bool {
        self.quiet.load(Ordering::SeqCst)
    }

    pub fn println(&self, msg: &str) {
        if self.is_quiet() {
            return;
        }
        let inner = self.inner.lock().unwrap();
        let _ = inner.multi.println(msg);
    }

    pub fn status(&self, verb: &str, msg: &str, color: &str) {
        if self.is_quiet() {
            return;
        }
        let styled_verb = match color {
            "cyan" => style(verb).cyan().bold(),
            "green" => style(verb).green().bold(),
            "red" => style(verb).red().bold(),
            "yellow" => style(verb).yellow().bold(),
            "white" => style(verb).white().bold(),
            _ => style(verb).bold(),
        };
        let line = format!("{:>12} {}", styled_verb, msg);
        self.println(&line);
    }

    pub fn header(&self, title: &str, subtitle: &str) {
        if self.is_quiet() {
            return;
        }
        self.println("");
        let line = format!("  {}  {}", style(title).white().bold(), subtitle);
        self.println(&line);
        self.println("");
    }

    pub fn task_starting(&self, name: &str) {
        if self.is_quiet() {
            return;
        }
        if self.is_tty {
            let mut inner = self.inner.lock().unwrap();
            let pb = inner.multi.add(ProgressBar::new_spinner());
            let style = ProgressStyle::default_spinner()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
                .template("  {spinner:.cyan} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner());
            pb.set_style(style);
            pb.set_message(format!("building  {}", name));
            pb.enable_steady_tick(std::time::Duration::from_millis(80));
            inner.spinners.insert(name.to_string(), pb);
        } else {
            self.status("Building", name, "cyan");
        }
    }

    fn finish_spinner(&self, name: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(pb) = inner.spinners.remove(name) {
            pb.finish_and_clear();
        }
    }

    pub fn task_cached(&self, name: &str) {
        self.finish_spinner(name);
        self.status("Cached", &format!("{} (skipped)", name), "cyan");
    }

    pub fn task_completed(&self, name: &str, duration: std::time::Duration) {
        self.finish_spinner(name);
        let time_str = format!("{:.1?}", duration);
        self.status("Completed", &format!("{} ({})", name, time_str), "green");
    }

    pub fn task_failed(&self, name: &str, error: &str) {
        self.finish_spinner(name);
        self.status("error", &format!("{}: {}", name, error), "red");
        self.status("Failed", name, "red");
    }

    pub fn task_rollback(&self, name: &str) {
        self.finish_spinner(name);
        self.status("Rollback", &format!("{} - reverting", name), "yellow");
    }

    pub fn summary(
        &self,
        duration: std::time::Duration,
        cached: usize,
        failed: usize,
        total: usize,
    ) {
        if self.is_quiet() {
            return;
        }
        let time_str = format!("{:.1?}", duration);
        let status_color = if failed > 0 { "red" } else { "green" };
        let msg = format!(
            "{} services in {} ({} cached, {} failed)",
            total, time_str, cached, failed
        );
        self.status("Finished", &msg, status_color);
    }

    pub fn error(&self, msg: &str) {
        let line = format!("{:>12} {}", style("error").red().bold(), msg);
        let inner = self.inner.lock().unwrap();
        let _ = inner.multi.println(line);
    }

    pub fn warn(&self, msg: &str) {
        if self.is_quiet() {
            return;
        }
        let line = format!("{:>12} {}", style("warning").yellow().bold(), msg);
        let inner = self.inner.lock().unwrap();
        let _ = inner.multi.println(line);
    }

    pub fn info(&self, msg: &str) {
        if self.is_quiet() {
            return;
        }
        let line = format!("{:>12} {}", style("info").cyan().bold(), msg);
        let inner = self.inner.lock().unwrap();
        let _ = inner.multi.println(line);
    }

    pub fn debug(&self, msg: &str) {
        if self.is_quiet() || !self.is_verbose() {
            return;
        }
        let line = format!("{:>12} {}", style("debug").magenta().bold(), msg);
        let inner = self.inner.lock().unwrap();
        let _ = inner.multi.println(line);
    }

    pub fn trace(&self, msg: &str) {
        if self.is_quiet() || !self.is_verbose() {
            return;
        }
        let line = format!("{:>12} {}", style("trace").dim(), msg);
        let inner = self.inner.lock().unwrap();
        let _ = inner.multi.println(line);
    }
}
