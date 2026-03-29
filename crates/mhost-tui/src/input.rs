use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, ConfirmAction};

// ---------------------------------------------------------------------------
// Action
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    None,
    Quit,
    NextTab,
    SelectNext,
    SelectPrev,
    SelectFirst,
    SelectLast,
    EnterSearch,
    /// Restart the selected process (confirmation already given).
    Restart,
    /// Stop the selected process (confirmation already given).
    Stop,
    /// Delete the selected process (confirmation already given).
    Delete,
    /// A confirmation prompt was just raised (waiting for y/n).
    ConfirmPending,
    /// The pending confirmation was cancelled.
    ConfirmCancelled,
    /// Sort column cycled.
    SortCycled,
    /// Sort direction toggled.
    SortToggled,
    /// Scroll log up.
    ScrollUp,
    /// Scroll log down.
    ScrollDown,
}

// ---------------------------------------------------------------------------
// handle_key
// ---------------------------------------------------------------------------

/// Translate a raw key event into an [`Action`], applying side-effects to
/// `app` where necessary (e.g. updating the search buffer, cycling tabs).
pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    // -- Confirmation dialog is open ------------------------------------------
    if let Some(confirm) = app.confirm_action.take() {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let action = match &confirm {
                    ConfirmAction::Restart(_) => Action::Restart,
                    ConfirmAction::Stop(_) => Action::Stop,
                    ConfirmAction::Delete(_) => Action::Delete,
                };
                // Keep the name in status message; caller will send IPC.
                app.set_status(format!("{}ing {}…", confirm.verb(), confirm.process_name()));
                return action;
            }
            _ => {
                // Any key other than 'y' cancels.
                app.set_status("Cancelled.");
                return Action::ConfirmCancelled;
            }
        }
    }

    // -- Search mode ----------------------------------------------------------
    if app.search_mode {
        match key.code {
            KeyCode::Esc => {
                app.search_mode = false;
                app.search_query = None;
            }
            KeyCode::Enter => {
                app.search_mode = false;
            }
            KeyCode::Char(c) => {
                app.search_query.get_or_insert_with(String::new).push(c);
            }
            KeyCode::Backspace => {
                if let Some(q) = &mut app.search_query {
                    q.pop();
                }
            }
            _ => {}
        }
        return Action::None;
    }

    // -- Normal mode ----------------------------------------------------------
    match key.code {
        // Quit
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,

        // Tab cycling
        KeyCode::Tab => {
            app.next_tab();
            Action::NextTab
        }

        // Navigation
        KeyCode::Char('j') | KeyCode::Down => {
            app.select_next();
            Action::SelectNext
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.select_prev();
            Action::SelectPrev
        }
        KeyCode::Char('g') => {
            app.select_first();
            Action::SelectFirst
        }
        KeyCode::Char('G') => {
            app.select_last();
            Action::SelectLast
        }

        // Log scrolling
        KeyCode::PageUp => {
            app.scroll_offset = app.scroll_offset.saturating_sub(10);
            Action::ScrollUp
        }
        KeyCode::PageDown => {
            app.scroll_offset = app.scroll_offset.saturating_add(10);
            Action::ScrollDown
        }

        // Search
        KeyCode::Char('/') => {
            app.search_mode = true;
            app.search_query = Some(String::new());
            Action::EnterSearch
        }

        // Process actions — raise confirmation
        KeyCode::Char('r') => {
            if let Some(name) = app.selected_process_name().map(String::from) {
                app.confirm_action = Some(ConfirmAction::Restart(name.clone()));
                app.set_status(format!("Restart '{name}'? (y/n)"));
                Action::ConfirmPending
            } else {
                Action::None
            }
        }
        KeyCode::Char('s') => {
            if let Some(name) = app.selected_process_name().map(String::from) {
                app.confirm_action = Some(ConfirmAction::Stop(name.clone()));
                app.set_status(format!("Stop '{name}'? (y/n)"));
                Action::ConfirmPending
            } else {
                Action::None
            }
        }
        KeyCode::Char('d') => {
            if let Some(name) = app.selected_process_name().map(String::from) {
                app.confirm_action = Some(ConfirmAction::Delete(name.clone()));
                app.set_status(format!("Delete '{name}'? (y/n)"));
                Action::ConfirmPending
            } else {
                Action::None
            }
        }

        // Sort controls
        KeyCode::Char('S') => {
            app.cycle_sort();
            Action::SortCycled
        }
        KeyCode::Char('1') => {
            app.sort_by = crate::app::SortColumn::Name;
            Action::SortCycled
        }
        KeyCode::Char('2') => {
            app.sort_by = crate::app::SortColumn::Cpu;
            Action::SortCycled
        }
        KeyCode::Char('3') => {
            app.sort_by = crate::app::SortColumn::Memory;
            Action::SortCycled
        }
        KeyCode::Char('4') => {
            app.sort_by = crate::app::SortColumn::Uptime;
            Action::SortCycled
        }
        KeyCode::Char('R') => {
            app.toggle_sort_direction();
            Action::SortToggled
        }

        _ => Action::None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Tab};
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use mhost_core::process::{ProcessConfig, ProcessInfo};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn ctrl_key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn make_process(name: &str) -> ProcessInfo {
        let mut config = ProcessConfig::default();
        config.name = name.to_string();
        ProcessInfo::new(config, 0)
    }

    // -- q quits ---------------------------------------------------------------

    #[test]
    fn test_q_returns_quit() {
        let mut app = App::new();
        let action = handle_key(&mut app, key(KeyCode::Char('q')));
        assert_eq!(action, Action::Quit);
    }

    #[test]
    fn test_ctrl_c_returns_quit() {
        let mut app = App::new();
        let action = handle_key(&mut app, ctrl_key(KeyCode::Char('c')));
        assert_eq!(action, Action::Quit);
    }

    // -- tab cycles ------------------------------------------------------------

    #[test]
    fn test_tab_key_cycles_tab() {
        let mut app = App::new();
        assert_eq!(app.current_tab, Tab::Processes);
        let action = handle_key(&mut app, key(KeyCode::Tab));
        assert_eq!(action, Action::NextTab);
        assert_eq!(app.current_tab, Tab::Logs);
    }

    // -- j/k selection ---------------------------------------------------------

    #[test]
    fn test_j_selects_next() {
        let mut app = App::new();
        app.processes = vec![make_process("a"), make_process("b")];
        let action = handle_key(&mut app, key(KeyCode::Char('j')));
        assert_eq!(action, Action::SelectNext);
        assert_eq!(app.selected_process, 1);
    }

    #[test]
    fn test_down_arrow_selects_next() {
        let mut app = App::new();
        app.processes = vec![make_process("a"), make_process("b")];
        let action = handle_key(&mut app, key(KeyCode::Down));
        assert_eq!(action, Action::SelectNext);
        assert_eq!(app.selected_process, 1);
    }

    #[test]
    fn test_k_selects_prev() {
        let mut app = App::new();
        app.processes = vec![make_process("a"), make_process("b")];
        app.selected_process = 1;
        let action = handle_key(&mut app, key(KeyCode::Char('k')));
        assert_eq!(action, Action::SelectPrev);
        assert_eq!(app.selected_process, 0);
    }

    #[test]
    fn test_up_arrow_selects_prev() {
        let mut app = App::new();
        app.processes = vec![make_process("a"), make_process("b")];
        app.selected_process = 1;
        let action = handle_key(&mut app, key(KeyCode::Up));
        assert_eq!(action, Action::SelectPrev);
        assert_eq!(app.selected_process, 0);
    }

    // -- g/G first/last --------------------------------------------------------

    #[test]
    fn test_g_selects_first() {
        let mut app = App::new();
        app.processes = vec![make_process("a"), make_process("b"), make_process("c")];
        app.selected_process = 2;
        let action = handle_key(&mut app, key(KeyCode::Char('g')));
        assert_eq!(action, Action::SelectFirst);
        assert_eq!(app.selected_process, 0);
    }

    #[test]
    fn test_shift_g_selects_last() {
        let mut app = App::new();
        app.processes = vec![make_process("a"), make_process("b"), make_process("c")];
        app.selected_process = 0;
        let action = handle_key(&mut app, key(KeyCode::Char('G')));
        assert_eq!(action, Action::SelectLast);
        assert_eq!(app.selected_process, 2);
    }

    // -- / enters search mode --------------------------------------------------

    #[test]
    fn test_slash_enters_search_mode() {
        let mut app = App::new();
        let action = handle_key(&mut app, key(KeyCode::Char('/')));
        assert_eq!(action, Action::EnterSearch);
        assert!(app.search_mode);
        assert_eq!(app.search_query, Some(String::new()));
    }

    // -- search mode key handling ----------------------------------------------

    #[test]
    fn test_search_mode_char_appends_to_query() {
        let mut app = App::new();
        app.search_mode = true;
        app.search_query = Some("foo".into());
        handle_key(&mut app, key(KeyCode::Char('d')));
        assert_eq!(app.search_query, Some("food".into()));
    }

    #[test]
    fn test_search_mode_backspace_removes_char() {
        let mut app = App::new();
        app.search_mode = true;
        app.search_query = Some("foo".into());
        handle_key(&mut app, key(KeyCode::Backspace));
        assert_eq!(app.search_query, Some("fo".into()));
    }

    #[test]
    fn test_search_mode_esc_clears_search() {
        let mut app = App::new();
        app.search_mode = true;
        app.search_query = Some("foo".into());
        handle_key(&mut app, key(KeyCode::Esc));
        assert!(!app.search_mode);
        assert_eq!(app.search_query, None);
    }

    #[test]
    fn test_search_mode_enter_exits_search_mode() {
        let mut app = App::new();
        app.search_mode = true;
        app.search_query = Some("foo".into());
        handle_key(&mut app, key(KeyCode::Enter));
        assert!(!app.search_mode);
        assert_eq!(app.search_query, Some("foo".into()));
    }

    #[test]
    fn test_search_mode_suppresses_normal_keys() {
        let mut app = App::new();
        app.processes = vec![make_process("a"), make_process("b")];
        app.search_mode = true;
        app.search_query = Some(String::new());
        // 'q' in search mode should type 'q', not quit
        let action = handle_key(&mut app, key(KeyCode::Char('q')));
        assert_eq!(action, Action::None);
        assert_eq!(app.search_query, Some("q".into()));
    }

    // -- process action confirmations -----------------------------------------

    #[test]
    fn test_r_raises_restart_confirmation() {
        let mut app = App::new();
        app.processes = vec![make_process("api")];
        let action = handle_key(&mut app, key(KeyCode::Char('r')));
        assert_eq!(action, Action::ConfirmPending);
        assert!(matches!(
            app.confirm_action,
            Some(ConfirmAction::Restart(_))
        ));
    }

    #[test]
    fn test_r_no_processes_is_noop() {
        let mut app = App::new();
        let action = handle_key(&mut app, key(KeyCode::Char('r')));
        assert_eq!(action, Action::None);
        assert!(app.confirm_action.is_none());
    }

    #[test]
    fn test_confirm_y_returns_restart() {
        let mut app = App::new();
        app.confirm_action = Some(ConfirmAction::Restart("api".into()));
        let action = handle_key(&mut app, key(KeyCode::Char('y')));
        assert_eq!(action, Action::Restart);
        assert!(app.confirm_action.is_none());
    }

    #[test]
    fn test_confirm_n_cancels() {
        let mut app = App::new();
        app.confirm_action = Some(ConfirmAction::Stop("api".into()));
        let action = handle_key(&mut app, key(KeyCode::Char('n')));
        assert_eq!(action, Action::ConfirmCancelled);
        assert!(app.confirm_action.is_none());
    }

    #[test]
    fn test_confirm_esc_cancels() {
        let mut app = App::new();
        app.confirm_action = Some(ConfirmAction::Delete("old".into()));
        let action = handle_key(&mut app, key(KeyCode::Esc));
        assert_eq!(action, Action::ConfirmCancelled);
    }

    #[test]
    fn test_s_raises_stop_confirmation() {
        let mut app = App::new();
        app.processes = vec![make_process("worker")];
        let action = handle_key(&mut app, key(KeyCode::Char('s')));
        assert_eq!(action, Action::ConfirmPending);
        assert!(matches!(app.confirm_action, Some(ConfirmAction::Stop(_))));
    }

    #[test]
    fn test_d_raises_delete_confirmation() {
        let mut app = App::new();
        app.processes = vec![make_process("old-svc")];
        let action = handle_key(&mut app, key(KeyCode::Char('d')));
        assert_eq!(action, Action::ConfirmPending);
        assert!(matches!(app.confirm_action, Some(ConfirmAction::Delete(_))));
    }

    // -- sort controls ---------------------------------------------------------

    #[test]
    fn test_shift_s_cycles_sort() {
        let mut app = App::new();
        let action = handle_key(&mut app, key(KeyCode::Char('S')));
        assert_eq!(action, Action::SortCycled);
        // Name → Status
        assert_eq!(app.sort_by, crate::app::SortColumn::Status);
    }

    #[test]
    fn test_digit_1_sets_sort_name() {
        let mut app = App::new();
        app.sort_by = crate::app::SortColumn::Cpu;
        handle_key(&mut app, key(KeyCode::Char('1')));
        assert_eq!(app.sort_by, crate::app::SortColumn::Name);
    }

    #[test]
    fn test_shift_r_toggles_sort_direction() {
        let mut app = App::new();
        assert!(app.sort_ascending);
        let action = handle_key(&mut app, key(KeyCode::Char('R')));
        assert_eq!(action, Action::SortToggled);
        assert!(!app.sort_ascending);
    }
}
