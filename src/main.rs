mod app;
mod ui;

use app::{App, SortKey};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
};
use std::io::{self, Stdout};
use std::time::Duration;

/// RAII guard that restores the terminal to its original state on drop.
///
/// This ensures the terminal is always cleaned up, even on early returns or panics
/// (when combined with the panic hook installed by [`install_panic_hook`]).
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

/// Installs a panic hook that restores the terminal before printing the panic message.
///
/// Without this, a panic would leave the terminal in raw mode with the alternate screen
/// active, making the shell unusable until manually reset.
fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));
}

fn run(guard: &mut TerminalGuard) -> io::Result<()> {
    let mut app = App::new();
    app.refresh_processes();

    let tick_interval = Duration::from_millis(1000);

    loop {
        guard.terminal.draw(|f| {
            let has_expanded = app.expanded_cmd.is_some();
            let system_height = app.system_stats.panel_height();

            let constraints: Vec<Constraint> = if has_expanded {
                vec![
                    Constraint::Length(system_height),
                    Constraint::Min(10),
                    Constraint::Length(10),
                    Constraint::Length(4),
                ]
            } else {
                vec![
                    Constraint::Length(system_height),
                    Constraint::Min(10),
                    Constraint::Length(4),
                ]
            };

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(f.area());

            // Compute visible rows from the process table area
            let table_area = chunks[1];
            app.visible_rows = table_area.height.saturating_sub(3) as usize;

            ui::render_system_bars(f, &app, chunks[0]);

            if has_expanded {
                ui::render_process_table(f, &app, chunks[1]);
                ui::render_command_panel(f, &app, chunks[2]);
                ui::render_help(f, chunks[3]);
            } else {
                ui::render_process_table(f, &app, chunks[1]);
                ui::render_help(f, chunks[2]);
            }
        })?;

        if event::poll(tick_interval)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('1') => app.toggle_sort(SortKey::Cpu),
                KeyCode::Char('2') => app.toggle_sort(SortKey::Mem),
                KeyCode::Char('3') => app.toggle_sort(SortKey::Pid),
                KeyCode::Char('4') => app.toggle_sort(SortKey::Name),
                KeyCode::Char('j') | KeyCode::Down => app.move_selection(1),
                KeyCode::Char('k') | KeyCode::Up => app.move_selection(-1),
                KeyCode::Enter => app.toggle_expand(),
                _ => {}
            }
        }

        app.refresh_processes();
    }

    Ok(())
}

fn main() -> io::Result<()> {
    install_panic_hook();
    let mut guard = TerminalGuard::new()?;
    run(&mut guard)
    // `guard` is dropped here, restoring the terminal via the `Drop` impl.
}
