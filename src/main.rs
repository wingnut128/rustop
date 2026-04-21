use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
};
use std::cmp::Ordering;
use std::time::Duration;
use sysinfo::System;

#[derive(PartialEq)]
enum SortKey {
    Cpu,
    Mem,
    Pid,
    Name,
}

struct ProcessInfo {
    pid: u32,
    name: String,
    cpu: f32,
    mem: f32,
    cmd: Vec<String>,
}

struct App {
    system: System,
    processes: Vec<ProcessInfo>,
    sort_key: SortKey,
    sort_asc: bool,
    selected_pid: Option<u32>,
    expanded_cmd: Option<u32>,
    scroll_offset: usize,
}

impl App {
    fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        Self {
            system,
            processes: Vec::new(),
            sort_key: SortKey::Cpu,
            sort_asc: false,
            selected_pid: None,
            expanded_cmd: None,
            scroll_offset: 0,
        }
    }

    fn refresh_processes(&mut self) {
        self.system.refresh_all();
        let mut new_processes: Vec<ProcessInfo> = self
            .system
            .processes()
            .iter()
            .map(|(pid, process)| {
                let cmd: Vec<String> = process
                    .cmd()
                    .iter()
                    .map(|s| s.to_string_lossy().to_string())
                    .collect();
                ProcessInfo {
                    pid: pid.as_u32(),
                    name: process.name().to_string_lossy().to_string(),
                    cpu: process.cpu_usage(),
                    mem: process.memory() as f32 / 1024.0 / 1024.0,
                    cmd,
                }
            })
            .collect();

        new_processes.sort_by(|a, b| match &self.sort_key {
            SortKey::Cpu => a.cpu.partial_cmp(&b.cpu).unwrap_or(Ordering::Equal),
            SortKey::Mem => a.mem.partial_cmp(&b.mem).unwrap_or(Ordering::Equal),
            SortKey::Pid => a.pid.cmp(&b.pid),
            SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });

        if !self.sort_asc {
            new_processes.reverse();
        }

        if let Some(selected) = self.selected_pid {
            self.selected_pid = new_processes
                .iter()
                .find(|p| p.pid == selected)
                .map(|p| p.pid);
        }

        self.processes = new_processes;
    }

    fn toggle_sort(&mut self, key: SortKey) {
        if self.sort_key == key {
            self.sort_asc = !self.sort_asc;
        } else {
            self.sort_key = key;
            self.sort_asc = false;
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.processes.is_empty() {
            return;
        }

        let current_idx = self
            .selected_pid
            .and_then(|pid| self.processes.iter().position(|p| p.pid == pid))
            .unwrap_or(0);

        let new_idx =
            ((current_idx as isize) + delta).clamp(0, (self.processes.len() - 1) as isize) as usize;
        self.selected_pid = Some(self.processes[new_idx].pid);

        if new_idx < self.scroll_offset {
            self.scroll_offset = new_idx;
        } else if new_idx >= self.scroll_offset + 20 {
            self.scroll_offset = new_idx.saturating_sub(19);
        }
    }
}

fn render_process_table(f: &mut Frame, app: &App, area: Rect) {
    let sort_indicator = |key: SortKey, label: &str| -> String {
        if app.sort_key == key {
            let arrow = if app.sort_asc { " ▲" } else { " ▼" };
            format!("{}{}", label, arrow)
        } else {
            label.to_string()
        }
    };

    let header = Row::new([
        sort_indicator(SortKey::Pid, "PID"),
        sort_indicator(SortKey::Name, "Process"),
        sort_indicator(SortKey::Cpu, "CPU%"),
        sort_indicator(SortKey::Mem, "MEM%"),
    ])
    .style(Style::default().fg(Color::Cyan).bold());

    let rows: Vec<Row> = app
        .processes
        .iter()
        .skip(app.scroll_offset)
        .take(20)
        .map(|p| {
            let is_selected = app.selected_pid == Some(p.pid);
            let is_expanded = app.expanded_cmd == Some(p.pid);
            let style = if is_selected {
                Style::default().bg(Color::Blue).fg(Color::White)
            } else {
                Style::default()
            };

            let name = if is_expanded && !p.cmd.is_empty() {
                p.cmd.join(" ")
            } else {
                p.name.clone()
            };

            Row::new([
                p.pid.to_string(),
                name,
                format!("{:.1}", p.cpu),
                format!("{:.1}", p.mem),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Min(20),
            Constraint::Length(8),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .block(Block::default().title("Processes").borders(Borders::ALL))
    .widths([
        Constraint::Length(10),
        Constraint::Min(20),
        Constraint::Length(8),
        Constraint::Length(8),
    ]);

    f.render_widget(table, area);
}

fn render_command_panel(f: &mut Frame, app: &App, area: Rect) {
    #[allow(clippy::collapsible_if)]
    if let Some(pid) = app.expanded_cmd
        && let Some(process) = app.processes.iter().find(|p| p.pid == pid)
        && !process.cmd.is_empty()
    {
        let cmd_str = process.cmd.join(" ");
        let cmd_lines: Vec<Line> = cmd_str
            .lines()
            .map(|line| Line::from(Span::raw(line.to_string())))
            .collect();

        let panel = Paragraph::new(cmd_lines)
            .block(
                Block::default()
                    .title(format!("Command (PID: {})", pid))
                    .borders(Borders::ALL),
            )
            .wrap(ratatui::widgets::Wrap { trim: false });

        f.render_widget(panel, area);
    }
}

fn render_help(f: &mut Frame, area: Rect) {
    let help_text = vec![
        Line::from(vec![
            Span::raw("Sort: "),
            Span::styled("1", Style::default().fg(Color::Yellow)),
            Span::raw("=CPU "),
            Span::styled("2", Style::default().fg(Color::Yellow)),
            Span::raw("=MEM "),
            Span::styled("3", Style::default().fg(Color::Yellow)),
            Span::raw("=PID "),
            Span::styled("4", Style::default().fg(Color::Yellow)),
            Span::raw("=Name"),
        ]),
        Line::from(vec![
            Span::raw("Navigate: "),
            Span::styled("↑/↓", Style::default().fg(Color::Yellow)),
            Span::raw(" or "),
            Span::styled("j/k", Style::default().fg(Color::Yellow)),
            Span::raw(" | "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw("=Expand/Collapse "),
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::raw("=Quit"),
        ]),
    ];

    let help = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::DarkGray));

    f.render_widget(help, area);
}

fn main() -> std::io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    app.refresh_processes();

    let tick_interval = Duration::from_millis(1000);

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(20),
                    Constraint::Length(10),
                    Constraint::Length(5),
                ])
                .split(f.area());

            render_process_table(f, &app, chunks[0]);
            render_command_panel(f, &app, chunks[1]);
            render_help(f, chunks[2]);
        })?;

        #[allow(clippy::collapsible_if)]
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
                KeyCode::Enter => {
                    if app.selected_pid == app.expanded_cmd {
                        app.expanded_cmd = None;
                    } else {
                        app.expanded_cmd = app.selected_pid;
                    }
                }
                _ => {}
            }
        }

        app.refresh_processes();
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
