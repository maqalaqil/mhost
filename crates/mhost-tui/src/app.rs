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
// App
// ---------------------------------------------------------------------------

pub struct App {
    pub current_tab: Tab,
    pub selected_process: usize,
    pub processes: Vec<ProcessInfo>,
    pub log_lines: Vec<String>,
    pub scroll_offset: u16,
    pub running: bool,
    pub search_query: Option<String>,
    pub search_mode: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            current_tab: Tab::Processes,
            selected_process: 0,
            processes: Vec::new(),
            log_lines: Vec::new(),
            scroll_offset: 0,
            running: true,
            search_query: None,
            search_mode: false,
        }
    }

    pub fn next_tab(&mut self) {
        self.current_tab = match self.current_tab {
            Tab::Processes => Tab::Logs,
            Tab::Logs => Tab::Metrics,
            Tab::Metrics => Tab::Proxy,
            Tab::Proxy => Tab::Processes,
        };
    }

    pub fn select_next(&mut self) {
        if !self.processes.is_empty() {
            self.selected_process =
                (self.selected_process + 1).min(self.processes.len() - 1);
        }
    }

    pub fn select_prev(&mut self) {
        self.selected_process = self.selected_process.saturating_sub(1);
    }

    pub fn select_first(&mut self) {
        self.selected_process = 0;
    }

    pub fn select_last(&mut self) {
        self.selected_process = self.processes.len().saturating_sub(1);
    }

    pub fn selected_process_name(&self) -> Option<&str> {
        self.processes
            .get(self.selected_process)
            .map(|p| p.config.name.as_str())
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
}
