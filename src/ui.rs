use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table, Wrap},
};

use crate::app::{App, SortKey};

/// Column widths used by the process table, defined once to avoid duplication.
const COLUMN_WIDTHS: [Constraint; 4] = [
    Constraint::Length(10),
    Constraint::Min(20),
    Constraint::Length(8),
    Constraint::Length(10),
];

/// Compute the number of visible data rows from the table area height.
/// Subtracts 2 for top/bottom borders and 1 for the header row.
pub fn visible_rows_for_area(area: Rect) -> usize {
    area.height.saturating_sub(3) as usize
}

/// Format bytes into a human-readable string with adaptive units.
fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    const TIB: f64 = GIB * 1024.0;

    let b = bytes as f64;
    if b >= TIB {
        format!("{:.2}T", b / TIB)
    } else if b >= GIB {
        format!("{:.2}G", b / GIB)
    } else if b >= MIB {
        format!("{:.0}M", b / MIB)
    } else {
        format!("{:.0}K", b / KIB)
    }
}

/// Build styled spans for a single CPU bar.
///
/// Format: `NN[||||||       ] XX.X%`
///
/// Bar colors: green (0–50%), yellow (50–80%), red (80–100%).
fn cpu_bar_spans(
    index: usize,
    usage: f32,
    bar_width: usize,
    label_width: usize,
) -> Vec<Span<'static>> {
    let label = format!("{:>width$}", index, width = label_width);
    let pct_label = format!("{:>5.1}%", usage);

    let filled = ((usage / 100.0) * bar_width as f32).round() as usize;
    let filled = filled.min(bar_width);

    let green_end = (bar_width as f32 * 0.5).round() as usize;
    let yellow_end = (bar_width as f32 * 0.8).round() as usize;

    let mut spans = Vec::with_capacity(bar_width + 4);
    spans.push(Span::styled(label, Style::default().fg(Color::Cyan)));
    spans.push(Span::styled("[", Style::default().fg(Color::DarkGray)));

    // Build three contiguous colored segments for efficiency
    let green_fill = filled.min(green_end);
    let yellow_fill = if filled > green_end {
        (filled - green_end).min(yellow_end - green_end)
    } else {
        0
    };
    let red_fill = filled.saturating_sub(yellow_end);
    let empty = bar_width - filled;

    if green_fill > 0 {
        spans.push(Span::styled(
            "|".repeat(green_fill),
            Style::default().fg(Color::Green),
        ));
    }
    if yellow_fill > 0 {
        spans.push(Span::styled(
            "|".repeat(yellow_fill),
            Style::default().fg(Color::Yellow),
        ));
    }
    if red_fill > 0 {
        spans.push(Span::styled(
            "|".repeat(red_fill),
            Style::default().fg(Color::Red),
        ));
    }
    if empty > 0 {
        spans.push(Span::raw(" ".repeat(empty)));
    }

    spans.push(Span::styled("]", Style::default().fg(Color::DarkGray)));
    spans.push(Span::styled(pct_label, Style::default().fg(Color::White)));

    spans
}

/// Build styled spans for a memory/swap bar.
///
/// Format: `Lbl[||||||       ] X.XXG/X.XXG`
///
/// Bar is a single color based on overall usage: green (<50%), yellow (50–80%), red (≥80%).
fn memory_bar_spans(label: &str, used: u64, total: u64, bar_width: usize) -> Vec<Span<'static>> {
    let pct = if total > 0 {
        used as f64 / total as f64 * 100.0
    } else {
        0.0
    };
    let info = format!(" {}/{}", format_bytes(used), format_bytes(total));

    let filled = ((pct / 100.0) * bar_width as f64).round() as usize;
    let filled = filled.min(bar_width);
    let empty = bar_width - filled;

    let bar_color = if pct < 50.0 {
        Color::Green
    } else if pct < 80.0 {
        Color::Yellow
    } else {
        Color::Red
    };

    let mut spans = Vec::with_capacity(6);
    spans.push(Span::styled(
        format!("{:<3}", label),
        Style::default().fg(Color::Cyan),
    ));
    spans.push(Span::styled("[", Style::default().fg(Color::DarkGray)));

    if filled > 0 {
        spans.push(Span::styled(
            "|".repeat(filled),
            Style::default().fg(bar_color),
        ));
    }
    if empty > 0 {
        spans.push(Span::raw(" ".repeat(empty)));
    }

    spans.push(Span::styled("]", Style::default().fg(Color::DarkGray)));
    spans.push(Span::styled(info, Style::default().fg(Color::White)));

    spans
}

/// Render the htop-style system resource panel: per-CPU bars in two columns,
/// plus memory and swap bars at full width.
pub fn render_system_bars(f: &mut Frame, app: &App, area: Rect) {
    let stats = &app.system_stats;
    let inner_width = area.width.saturating_sub(2) as usize; // minus left/right borders
    let num_cpus = stats.cpus.len();

    if num_cpus == 0 || inner_width < 20 {
        // Too narrow or no CPU data — render an empty block
        let block = Block::default().title("System").borders(Borders::ALL);
        f.render_widget(block, area);
        return;
    }

    let half = num_cpus.div_ceil(2);
    let label_width = num_cpus.to_string().len();
    // Overhead per bar: label_width + "[" + "]" + " NNN.N%" = label_width + 8
    let bar_overhead = label_width + 8;
    // Two columns with 1-char gap
    let col_width = if num_cpus > 1 {
        inner_width / 2
    } else {
        inner_width
    };
    let cpu_bar_width = col_width.saturating_sub(bar_overhead).max(4);

    // Memory/swap overhead: "Lbl" (3) + "[" + "]" + " XXXX/XXXX" (~14) = ~19
    let mem_bar_width = inner_width.saturating_sub(19).max(4);

    let mut lines: Vec<Line> = Vec::new();

    // CPU bars in two columns
    for row in 0..half {
        let left_idx = row;
        let right_idx = row + half;

        let mut spans = cpu_bar_spans(left_idx, stats.cpus[left_idx], cpu_bar_width, label_width);

        if right_idx < num_cpus && num_cpus > 1 {
            spans.push(Span::raw(" "));
            spans.extend(cpu_bar_spans(
                right_idx,
                stats.cpus[right_idx],
                cpu_bar_width,
                label_width,
            ));
        }

        lines.push(Line::from(spans));
    }

    // Memory bar
    lines.push(Line::from(memory_bar_spans(
        "Mem",
        stats.mem_used,
        stats.mem_total,
        mem_bar_width,
    )));

    // Swap bar (only if swap is configured)
    if stats.swap_total > 0 {
        lines.push(Line::from(memory_bar_spans(
            "Swp",
            stats.swap_used,
            stats.swap_total,
            mem_bar_width,
        )));
    }

    let block = Block::default().title("System").borders(Borders::ALL);
    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

pub fn render_process_table(f: &mut Frame, app: &App, area: Rect) {
    let visible_rows = visible_rows_for_area(area);

    let sort_indicator = |key: SortKey, label: &str| -> String {
        if app.sort_key == key {
            let arrow = if app.sort_asc { " ▲" } else { " ▼" };
            format!("{label}{arrow}")
        } else {
            label.to_string()
        }
    };

    let header = Row::new([
        sort_indicator(SortKey::Pid, "PID"),
        sort_indicator(SortKey::Name, "Process"),
        sort_indicator(SortKey::Cpu, "CPU%"),
        sort_indicator(SortKey::Mem, "MEM (MiB)"),
    ])
    .style(Style::default().fg(Color::Cyan).bold());

    let rows: Vec<Row> = app
        .processes
        .iter()
        .skip(app.scroll_offset)
        .take(visible_rows)
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
                format!("{:.1}", p.mem_mib),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(rows, COLUMN_WIDTHS)
        .header(header)
        .block(Block::default().title("Processes").borders(Borders::ALL));

    f.render_widget(table, area);
}

pub fn render_command_panel(f: &mut Frame, app: &App, area: Rect) {
    let (title, cmd_lines) = match app.expanded_cmd {
        Some(pid) => match app.processes.iter().find(|p| p.pid == pid) {
            Some(process) if !process.cmd.is_empty() => {
                let cmd_str = process.cmd.join(" ");
                let lines: Vec<Line> = cmd_str
                    .lines()
                    .map(|line| Line::from(Span::raw(line.to_string())))
                    .collect();
                (format!("Command (PID: {pid})"), lines)
            }
            _ => ("Command".to_string(), vec![]),
        },
        None => ("Command".to_string(), vec![]),
    };

    let panel = Paragraph::new(cmd_lines)
        .block(Block::default().title(title).borders(Borders::ALL))
        .wrap(Wrap { trim: false });

    f.render_widget(panel, area);
}

pub fn render_help(f: &mut Frame, area: Rect) {
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
