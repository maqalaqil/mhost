use std::collections::HashMap;

use mhost_core::process::ProcessInfo;

// ---------------------------------------------------------------------------
// Tab
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Tab {
    Processes,
    Logs,
    Metrics,
    Proxy,
}

// ---------------------------------------------------------------------------
// SortColumn
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum SortColumn {
    Name,
    Status,
    Cpu,
    Memory,
    Uptime,
    Restarts,
}

impl SortColumn {
    /// Cycle to the next sort column.
    pub fn next(&self) -> Self {
        match self {
            SortColumn::Name => SortColumn::Status,
            SortColumn::Status => SortColumn::Cpu,
            SortColumn::Cpu => SortColumn::Memory,
            SortColumn::Memory => SortColumn::Uptime,
            SortColumn::Uptime => SortColumn::Restarts,
            SortColumn::Restarts => SortColumn::Name,
        }
    }

    /// Short display label shown in the column header.
    pub fn label(&self) -> &'static str {
        match self {
            SortColumn::Name => "Name",
            SortColumn::Status => "Status",
            SortColumn::Cpu => "CPU%",
            SortColumn::Memory => "Memory",
            SortColumn::Uptime => "Uptime",
            SortColumn::Restarts => "↺",
        }
    }
}

// ---------------------------------------------------------------------------
// ConfirmAction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmAction {
    Stop(String),
    Delete(String),
    Restart(String),
}

impl ConfirmAction {
    /// Human-readable verb for the confirmation prompt.
    pub fn verb(&self) -> &'static str {
        match self {
            ConfirmAction::Stop(_) => "stop",
            ConfirmAction::Delete(_) => "delete",
            ConfirmAction::Restart(_) => "restart",
        }
    }

    /// The process name this action targets.
    pub fn process_name(&self) -> &str {
        match self {
            ConfirmAction::Stop(n) | ConfirmAction::Delete(n) | ConfirmAction::Restart(n) => n,
        }
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

pub struct App {
    pub current_tab: Tab,
    pub selected_process: usize,
    pub processes: Vec<ProcessInfo>,
    pub log_lines: Vec<String>,
    /// CPU history: process name → last 60 samples (percentage).
    pub cpu_history: HashMap<String, Vec<f64>>,
    /// Memory history: process name → last 60 samples (MB).
    pub mem_history: HashMap<String, Vec<f64>>,
    pub scroll_offset: u16,
    pub running: bool,
    pub search_query: Option<String>,
    pub search_mode: bool,
    /// Pending confirmation for a destructive action.
    pub confirm_action: Option<ConfirmAction>,
    pub sort_by: SortColumn,
    pub sort_ascending: bool,
    /// Transient status message shown in the footer (message + timestamp).
    pub status_message: Option<(String, std::time::Instant)>,
    pub tick_count: u64,
}

impl App {
    pub fn new() -> Self {
        Self {
            current_tab: Tab::Processes,
            selected_process: 0,
            processes: Vec::new(),
            log_lines: Vec::new(),
            cpu_history: HashMap::new(),
            mem_history: HashMap::new(),
            scroll_offset: 0,
            running: true,
            search_query: None,
            search_mode: false,
            confirm_action: None,
            sort_by: SortColumn::Name,
            sort_ascending: true,
            status_message: None,
            tick_count: 0,
        }
    }

    // -- Tab cycling ----------------------------------------------------------

    pub fn next_tab(&mut self) {
        self.current_tab = match self.current_tab {
            Tab::Processes => Tab::Logs,
            Tab::Logs => Tab::Metrics,
            Tab::Metrics => Tab::Proxy,
            Tab::Proxy => Tab::Processes,
        };
    }

    // -- Process selection ----------------------------------------------------

    pub fn select_next(&mut self) {
        let count = self.filtered_processes().len();
        if count > 0 {
            self.selected_process = (self.selected_process + 1).min(count - 1);
        }
    }

    pub fn select_prev(&mut self) {
        self.selected_process = self.selected_process.saturating_sub(1);
    }

    pub fn select_first(&mut self) {
        self.selected_process = 0;
    }

    pub fn select_last(&mut self) {
        let count = self.filtered_processes().len();
        self.selected_process = count.saturating_sub(1);
    }

    /// Name of the currently highlighted process (in the filtered+sorted view).
    pub fn selected_process_name(&self) -> Option<&str> {
        self.processes
            .get(self.selected_process)
            .map(|p| p.config.name.as_str())
    }

    // -- Sorting --------------------------------------------------------------

    /// Cycle to the next sort column.
    pub fn cycle_sort(&mut self) {
        self.sort_by = self.sort_by.next();
    }

    /// Toggle ascending / descending.
    pub fn toggle_sort_direction(&mut self) {
        self.sort_ascending = !self.sort_ascending;
    }

    /// Return references to processes after applying the search filter, then
    /// sorting.  The indices are into `self.processes`.
    pub fn sorted_processes(&self) -> Vec<&ProcessInfo> {
        let mut procs: Vec<&ProcessInfo> = self
            .processes
            .iter()
            .filter(|p| self.matches_search(p))
            .collect();

        procs.sort_by(|a, b| {
            let ord = match &self.sort_by {
                SortColumn::Name => a.config.name.cmp(&b.config.name),
                SortColumn::Status => a.status.to_string().cmp(&b.status.to_string()),
                SortColumn::Cpu => a
                    .cpu_percent
                    .unwrap_or(0.0)
                    .partial_cmp(&b.cpu_percent.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::Memory => a
                    .memory_bytes
                    .unwrap_or(0)
                    .cmp(&b.memory_bytes.unwrap_or(0)),
                SortColumn::Uptime => a
                    .uptime_seconds()
                    .unwrap_or(0)
                    .cmp(&b.uptime_seconds().unwrap_or(0)),
                SortColumn::Restarts => a.restart_count.cmp(&b.restart_count),
            };
            if self.sort_ascending {
                ord
            } else {
                ord.reverse()
            }
        });

        procs
    }

    /// Alias for `sorted_processes` — used to keep old call-sites working.
    pub fn filtered_processes(&self) -> Vec<&ProcessInfo> {
        self.sorted_processes()
    }

    fn matches_search(&self, p: &ProcessInfo) -> bool {
        match &self.search_query {
            None => true,
            Some(q) if q.is_empty() => true,
            Some(q) => p.config.name.to_lowercase().contains(&q.to_lowercase()),
        }
    }

    // -- Metrics recording ----------------------------------------------------

    const MAX_HISTORY: usize = 60;

    /// Push the latest CPU and memory samples from `self.processes` into the
    /// history maps.  Caps each vec at `MAX_HISTORY` entries.
    pub fn record_metrics(&mut self) {
        // Collect updates first to avoid borrow conflict with self.cpu_history.
        let updates: Vec<(String, f64, f64)> = self
            .processes
            .iter()
            .map(|p| {
                let cpu = p.cpu_percent.unwrap_or(0.0) as f64;
                let mem = p.memory_bytes.unwrap_or(0) as f64 / 1_048_576.0;
                (p.config.name.clone(), cpu, mem)
            })
            .collect();

        for (name, cpu, mem) in updates {
            let cpu_vec = self.cpu_history.entry(name.clone()).or_default();
            cpu_vec.push(cpu);
            if cpu_vec.len() > Self::MAX_HISTORY {
                let drain_count = cpu_vec.len() - Self::MAX_HISTORY;
                cpu_vec.drain(..drain_count);
            }

            let mem_vec = self.mem_history.entry(name).or_default();
            mem_vec.push(mem);
            if mem_vec.len() > Self::MAX_HISTORY {
                let drain_count = mem_vec.len() - Self::MAX_HISTORY;
                mem_vec.drain(..drain_count);
            }
        }
    }

    // -- Status message -------------------------------------------------------

    /// Set a transient flash message in the footer.
    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), std::time::Instant::now()));
    }

    /// Clear the status message if it has been shown for more than 3 seconds.
    pub fn expire_status(&mut self) {
        if let Some((_, ts)) = &self.status_message {
            if ts.elapsed().as_secs() >= 3 {
                self.status_message = None;
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use mhost_core::process::{ProcessConfig, ProcessInfo};

    fn make_process(name: &str) -> ProcessInfo {
        let mut config = ProcessConfig::default();
        config.name = name.to_string();
        ProcessInfo::new(config, 0)
    }

    #[test]
    fn test_new_app_defaults() {
        let app = App::new();
        assert_eq!(app.current_tab, Tab::Processes);
        assert_eq!(app.selected_process, 0);
        assert!(app.processes.is_empty());
        assert!(app.log_lines.is_empty());
        assert_eq!(app.scroll_offset, 0);
        assert!(app.running);
        assert!(app.search_query.is_none());
        assert!(!app.search_mode);
    }

    #[test]
    fn test_tab_cycling() {
        let mut app = App::new();
        assert_eq!(app.current_tab, Tab::Processes);
        app.next_tab();
        assert_eq!(app.current_tab, Tab::Logs);
        app.next_tab();
        assert_eq!(app.current_tab, Tab::Metrics);
        app.next_tab();
        assert_eq!(app.current_tab, Tab::Proxy);
        app.next_tab();
        assert_eq!(app.current_tab, Tab::Processes);
    }

    #[test]
    fn test_select_next_clamps_at_end() {
        let mut app = App::new();
        app.processes = vec![make_process("a"), make_process("b"), make_process("c")];
        app.selected_process = 2;
        app.select_next();
        assert_eq!(app.selected_process, 2, "should not go past last index");
    }

    #[test]
    fn test_select_prev_clamps_at_zero() {
        let mut app = App::new();
        app.processes = vec![make_process("a"), make_process("b")];
        app.selected_process = 0;
        app.select_prev();
        assert_eq!(app.selected_process, 0, "should not go below zero");
    }

    #[test]
    fn test_select_next_advances() {
        let mut app = App::new();
        app.processes = vec![make_process("a"), make_process("b"), make_process("c")];
        app.selected_process = 0;
        app.select_next();
        assert_eq!(app.selected_process, 1);
    }

    #[test]
    fn test_select_first_and_last() {
        let mut app = App::new();
        app.processes = vec![make_process("a"), make_process("b"), make_process("c")];
        app.selected_process = 1;
        app.select_first();
        assert_eq!(app.selected_process, 0);
        app.select_last();
        assert_eq!(app.selected_process, 2);
    }

    #[test]
    fn test_select_next_on_empty_list() {
        let mut app = App::new();
        app.select_next(); // should not panic
        assert_eq!(app.selected_process, 0);
    }

    #[test]
    fn test_selected_process_name() {
        let mut app = App::new();
        app.processes = vec![make_process("alpha"), make_process("beta")];
        app.selected_process = 0;
        assert_eq!(app.selected_process_name(), Some("alpha"));
        app.selected_process = 1;
        assert_eq!(app.selected_process_name(), Some("beta"));
    }

    #[test]
    fn test_selected_process_name_empty() {
        let app = App::new();
        assert_eq!(app.selected_process_name(), None);
    }

    // -- new state fields ------------------------------------------------------

    #[test]
    fn test_new_app_has_empty_history() {
        let app = App::new();
        assert!(app.cpu_history.is_empty());
        assert!(app.mem_history.is_empty());
    }

    #[test]
    fn test_new_app_no_confirm_action() {
        let app = App::new();
        assert!(app.confirm_action.is_none());
    }

    #[test]
    fn test_new_app_sort_defaults() {
        let app = App::new();
        assert_eq!(app.sort_by, SortColumn::Name);
        assert!(app.sort_ascending);
    }

    // -- record_metrics --------------------------------------------------------

    #[test]
    fn test_record_metrics_populates_history() {
        let mut app = App::new();
        app.processes = vec![make_process("web")];
        app.processes[0].cpu_percent = Some(12.5);
        app.processes[0].memory_bytes = Some(128 * 1_048_576);
        app.record_metrics();
        assert_eq!(app.cpu_history["web"], vec![12.5_f64]);
        assert_eq!(app.mem_history["web"], vec![128.0_f64]);
    }

    #[test]
    fn test_record_metrics_caps_at_60() {
        let mut app = App::new();
        app.processes = vec![make_process("svc")];
        for i in 0..70 {
            app.processes[0].cpu_percent = Some(i as f32);
            app.record_metrics();
        }
        assert_eq!(app.cpu_history["svc"].len(), 60);
        // The oldest entries (0-9) should have been dropped.
        assert_eq!(app.cpu_history["svc"][0], 10.0);
    }

    // -- set_status / expire_status -------------------------------------------

    #[test]
    fn test_set_status_stores_message() {
        let mut app = App::new();
        app.set_status("hello");
        assert!(app.status_message.is_some());
        let (msg, _) = app.status_message.as_ref().unwrap();
        assert_eq!(msg, "hello");
    }

    #[test]
    fn test_expire_status_leaves_fresh_message() {
        let mut app = App::new();
        app.set_status("fresh");
        app.expire_status();
        assert!(app.status_message.is_some(), "fresh message should survive");
    }

    // -- sorted_processes ------------------------------------------------------

    #[test]
    fn test_sorted_processes_by_name_ascending() {
        let mut app = App::new();
        app.processes = vec![make_process("zebra"), make_process("alpha"), make_process("mango")];
        app.sort_by = SortColumn::Name;
        app.sort_ascending = true;
        let names: Vec<&str> = app.sorted_processes().iter().map(|p| p.config.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "mango", "zebra"]);
    }

    #[test]
    fn test_sorted_processes_by_name_descending() {
        let mut app = App::new();
        app.processes = vec![make_process("alpha"), make_process("zebra")];
        app.sort_by = SortColumn::Name;
        app.sort_ascending = false;
        let names: Vec<&str> = app.sorted_processes().iter().map(|p| p.config.name.as_str()).collect();
        assert_eq!(names, vec!["zebra", "alpha"]);
    }

    // -- search filter ---------------------------------------------------------

    #[test]
    fn test_filtered_processes_with_query() {
        let mut app = App::new();
        app.processes = vec![make_process("api-server"), make_process("worker"), make_process("api-proxy")];
        app.search_query = Some("api".into());
        let names: Vec<&str> = app.filtered_processes().iter().map(|p| p.config.name.as_str()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"api-server"));
        assert!(names.contains(&"api-proxy"));
    }

    // -- cycle_sort -----------------------------------------------------------

    #[test]
    fn test_cycle_sort_advances_column() {
        let mut app = App::new();
        assert_eq!(app.sort_by, SortColumn::Name);
        app.cycle_sort();
        assert_eq!(app.sort_by, SortColumn::Status);
    }

    // -- confirm_action -------------------------------------------------------

    #[test]
    fn test_confirm_action_verb_and_name() {
        let action = ConfirmAction::Restart("my-svc".into());
        assert_eq!(action.verb(), "restart");
        assert_eq!(action.process_name(), "my-svc");

        let action = ConfirmAction::Stop("worker".into());
        assert_eq!(action.verb(), "stop");

        let action = ConfirmAction::Delete("old-proc".into());
        assert_eq!(action.verb(), "delete");
    }
}
