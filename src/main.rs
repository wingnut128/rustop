mod app;
mod ui;

use app::{App, SortKey};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
};
use signal_hook::{
    consts::signal::{SIGHUP, SIGTERM},
    flag,
};
use std::{
    env,
    io::{self, Stdout},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn new() -> io::Result<Self> {
        enable_raw_mode().map_err(|error| {
            io::Error::new(error.kind(), format!("failed to enable raw mode: {error}"))
        })?;

        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(io::Error::new(
                error.kind(),
                format!("failed to enter the alternate screen: {error}"),
            ));
        }

        let backend = CrosstermBackend::new(stdout);
        match Terminal::new(backend) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                let _ = disable_raw_mode();
                let _ = execute!(io::stdout(), LeaveAlternateScreen);
                Err(io::Error::new(
                    error.kind(),
                    format!("failed to initialize the terminal: {error}"),
                ))
            }
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));
}

fn parse_refresh_interval(arguments: impl IntoIterator<Item = String>) -> Result<Duration, String> {
    let mut arguments = arguments.into_iter();
    let _program = arguments.next();
    let mut interval = DEFAULT_REFRESH_INTERVAL;

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--interval" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--interval requires a value in milliseconds".to_string())?;
                let milliseconds: u64 = value.parse().map_err(|_| {
                    format!("invalid --interval value {value:?}; expected a positive integer in milliseconds")
                })?;
                if milliseconds == 0 {
                    return Err("--interval must be greater than zero milliseconds".to_string());
                }
                interval = Duration::from_millis(milliseconds);
            }
            "--help" | "-h" => {
                return Err("Usage: rustop [--interval <milliseconds>]".to_string());
            }
            _ => {
                return Err(format!(
                    "unknown argument {argument:?}\nUsage: rustop [--interval <milliseconds>]"
                ));
            }
        }
    }
    Ok(interval)
}

fn install_termination_handler() -> io::Result<Arc<AtomicBool>> {
    let requested = Arc::new(AtomicBool::new(false));
    flag::register(SIGTERM, Arc::clone(&requested))?;
    flag::register(SIGHUP, Arc::clone(&requested))?;
    Ok(requested)
}

fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    if app.filtering {
        match key.code {
            KeyCode::Esc => app.clear_filter(),
            KeyCode::Enter => app.end_filter(),
            KeyCode::Backspace => app.pop_filter_char(),
            KeyCode::Char(character) => app.push_filter_char(character),
            _ => {}
        }
        return false;
    }

    match key.code {
        KeyCode::Char('q') => true,
        KeyCode::Esc if !app.filter.is_empty() => {
            app.clear_filter();
            false
        }
        KeyCode::Esc => true,
        KeyCode::Char('/') => {
            app.begin_filter();
            false
        }
        KeyCode::Char('1') => {
            app.toggle_sort(SortKey::Cpu);
            false
        }
        KeyCode::Char('2') => {
            app.toggle_sort(SortKey::Mem);
            false
        }
        KeyCode::Char('3') => {
            app.toggle_sort(SortKey::Pid);
            false
        }
        KeyCode::Char('4') => {
            app.toggle_sort(SortKey::Name);
            false
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.move_selection(1);
            false
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.move_selection(-1);
            false
        }
        KeyCode::Char('g') | KeyCode::Home => {
            app.move_to_edge(false);
            false
        }
        KeyCode::Char('G') | KeyCode::End => {
            app.move_to_edge(true);
            false
        }
        KeyCode::PageDown => {
            app.move_selection(app.visible_rows.max(1) as isize);
            false
        }
        KeyCode::PageUp => {
            app.move_selection(-(app.visible_rows.max(1) as isize));
            false
        }
        KeyCode::Char('n') => {
            app.move_to_match(true);
            false
        }
        KeyCode::Char('N') => {
            app.move_to_match(false);
            false
        }
        KeyCode::Char(' ') => {
            app.toggle_pause();
            false
        }
        KeyCode::Enter => {
            app.toggle_expand();
            false
        }
        _ => false,
    }
}

fn run(
    guard: &mut TerminalGuard,
    tick_interval: Duration,
    termination_requested: Arc<AtomicBool>,
) -> io::Result<()> {
    let mut app = App::new();
    app.refresh_processes();

    loop {
        guard.terminal.draw(|f| {
            let has_expanded = app.expanded_cmd.is_some();
            let system_height = app.system_stats.panel_height();
            let help_height = 6;
            let constraints = if has_expanded {
                vec![
                    Constraint::Length(system_height),
                    Constraint::Min(10),
                    Constraint::Length(10),
                    Constraint::Length(help_height),
                ]
            } else {
                vec![
                    Constraint::Length(system_height),
                    Constraint::Min(10),
                    Constraint::Length(help_height),
                ]
            };
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(f.area());
            let table_area = chunks[1];
            app.visible_rows = ui::visible_rows_for_area(table_area);
            ui::render_system_bars(f, &app, chunks[0]);
            ui::render_process_table(f, &app, table_area);
            if has_expanded {
                ui::render_command_panel(f, &app, chunks[2]);
                ui::render_help(f, &app, chunks[3]);
            } else {
                ui::render_help(f, &app, chunks[2]);
            }
        })?;

        if termination_requested.load(Ordering::Relaxed) {
            break;
        }
        if event::poll(tick_interval)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && handle_key(&mut app, key)
        {
            break;
        }
        if !app.paused {
            app.refresh_processes();
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let interval = parse_refresh_interval(env::args())
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
    let termination_requested = install_termination_handler()?;
    install_panic_hook();
    let mut guard = TerminalGuard::new()?;
    run(&mut guard, interval, termination_requested)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_interval_and_rejects_invalid_values() {
        assert_eq!(
            parse_refresh_interval(["rustop".into(), "--interval".into(), "250".into()]).unwrap(),
            Duration::from_millis(250)
        );
        assert!(
            parse_refresh_interval(["rustop".into(), "--interval".into(), "0".into()]).is_err()
        );
        assert!(
            parse_refresh_interval(["rustop".into(), "--interval".into(), "fast".into()]).is_err()
        );
    }
}
