use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Row, Table, Wrap},
};

use crate::app::{App, SortKey, sanitize};

const ACCENT: Color = Color::Cyan;
const SECONDARY: Color = Color::Magenta;
const MUTED: Color = Color::DarkGray;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DashboardMode {
    Wide,
    Narrow,
}

fn dashboard_mode(area: Rect) -> DashboardMode {
    if area.width >= 100 && area.height >= 28 {
        DashboardMode::Wide
    } else {
        DashboardMode::Narrow
    }
}

fn cpu_panel_height(cpu_count: usize, expanded: bool) -> u16 {
    if expanded {
        cpu_count.div_ceil(2).saturating_add(4).max(5) as u16
    } else {
        5
    }
}

fn sparkline_levels(samples: &[u64]) -> Vec<u64> {
    let max = samples.iter().copied().max().unwrap_or(0);
    if max == 0 {
        return vec![0; samples.len()];
    }
    samples
        .iter()
        .map(|sample| {
            if *sample == 0 {
                0
            } else {
                (sample.saturating_mul(8) / max).max(1)
            }
        })
        .collect()
}

fn sparkline_text(samples: &[u64], width: usize) -> String {
    const LEVELS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let start = samples.len().saturating_sub(width);
    sparkline_levels(&samples[start..])
        .into_iter()
        .map(|level| LEVELS[level.min(8) as usize])
        .collect()
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [(&str, f64); 5] = [
        ("TiB", 1_099_511_627_776.0),
        ("GiB", 1_073_741_824.0),
        ("MiB", 1_048_576.0),
        ("KiB", 1024.0),
        ("B", 1.0),
    ];
    let bytes = bytes as f64;
    let (unit, divisor) = UNITS
        .into_iter()
        .find(|(_, divisor)| bytes >= *divisor)
        .unwrap_or(("B", 1.0));
    format!("{:.2} {unit}", bytes / divisor)
}

fn format_rate(bytes: u64) -> String {
    format!("{}/s", format_bytes(bytes))
}

fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours:02}h")
    } else {
        format!("{hours:02}h {minutes:02}m")
    }
}

fn is_link_local_ipv6(address: &str) -> bool {
    address
        .split('/')
        .next()
        .and_then(|address| address.parse::<std::net::Ipv6Addr>().ok())
        .is_some_and(|address| address.is_unicast_link_local())
}

fn visible_ipv6_addresses(addresses: &[String], limit: usize) -> Vec<&str> {
    let mut selected = Vec::with_capacity(limit.min(addresses.len()));
    if let Some(index) = addresses
        .iter()
        .position(|address| !is_link_local_ipv6(address))
    {
        selected.push(index);
    }
    if selected.len() < limit
        && let Some(index) = addresses
            .iter()
            .position(|address| is_link_local_ipv6(address))
    {
        selected.push(index);
    }
    for index in 0..addresses.len() {
        if selected.len() == limit {
            break;
        }
        if !selected.contains(&index) {
            selected.push(index);
        }
    }
    selected
        .into_iter()
        .map(|index| addresses[index].as_str())
        .collect()
}

fn usage_color(percent: f64) -> Color {
    if percent >= 80.0 {
        Color::Red
    } else if percent >= 50.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}

fn panel<'a>(title: impl Into<Line<'a>>) -> Block<'a> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MUTED))
}

pub fn visible_rows_for_area(area: Rect) -> usize {
    area.height.saturating_sub(3) as usize
}

pub fn render_dashboard(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let mode = dashboard_mode(area);
    let header_height = if area.height >= 16 { 3 } else { 1 };
    let footer_height = if area.height >= 18 { 4 } else { 2 };
    let detail_height = if app.expanded_cmd.is_some() && area.height >= 24 {
        7
    } else {
        0
    };
    let desired_overview = match mode {
        DashboardMode::Wide => cpu_panel_height(app.system_stats.cpus.len(), app.cpu_expanded)
            .max(if app.network_visible { 10 } else { 7 }),
        DashboardMode::Narrow if area.height >= 36 => {
            cpu_panel_height(app.system_stats.cpus.len(), app.cpu_expanded).min(10)
                + if app.network_visible { 15 } else { 6 }
        }
        DashboardMode::Narrow => 8,
    };
    let max_overview = area
        .height
        .saturating_sub(header_height + footer_height + detail_height + 6);
    let overview_height = desired_overview.min(max_overview).max(3);

    let mut constraints = vec![
        Constraint::Length(header_height),
        Constraint::Length(overview_height),
        Constraint::Min(6),
    ];
    if detail_height > 0 {
        constraints.push(Constraint::Length(detail_height));
    }
    constraints.push(Constraint::Length(footer_height));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    render_header(f, app, chunks[0]);
    render_overview(f, app, chunks[1], mode);
    app.visible_rows = visible_rows_for_area(chunks[2]);
    render_process_table(f, app, chunks[2]);
    let footer_index = chunks.len() - 1;
    if detail_height > 0 {
        render_command_panel(f, app, chunks[3]);
    }
    render_help(f, app, chunks[footer_index]);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let status = if app.paused { " PAUSED " } else { " LIVE " };
    let status_color = if app.paused {
        Color::Yellow
    } else {
        Color::Green
    };
    let direction = if app.sort_asc { "ASC" } else { "DESC" };
    let filter = if app.filter.is_empty() {
        "all".to_string()
    } else {
        format!("/{}", app.filter)
    };
    let line = Line::from(vec![
        Span::styled(
            " RUSTOP ",
            Style::default()
                .fg(Color::Black)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  SYSTEM MONITOR", Style::default().fg(SECONDARY).bold()),
        Span::raw("   "),
        Span::styled(
            status,
            Style::default().fg(Color::Black).bg(status_color).bold(),
        ),
        Span::styled(
            format!(
                "  TASKS {}  RUN {}  SORT {:?} {direction}  FILTER {filter}",
                app.processes.len(),
                app.system_stats.running_processes,
                app.sort_key
            ),
            Style::default().fg(Color::Gray),
        ),
    ]);
    let widget = Paragraph::new(line)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(ACCENT)),
        )
        .alignment(Alignment::Left);
    f.render_widget(widget, area);
}

fn render_overview(f: &mut Frame, app: &App, area: Rect, mode: DashboardMode) {
    if mode == DashboardMode::Narrow && app.network_visible && area.height >= 16 {
        let cpu_height = cpu_panel_height(app.system_stats.cpus.len(), app.cpu_expanded)
            .min(area.height.saturating_sub(14))
            .max(3);
        let rows = Layout::vertical([
            Constraint::Length(cpu_height),
            Constraint::Length(6),
            Constraint::Min(8),
        ])
        .split(area);
        render_system_bars(f, app, rows[0]);
        render_memory_panel(f, app, rows[1]);
        render_network_panel(f, app, rows[2]);
    } else if mode == DashboardMode::Narrow && area.height >= 13 {
        let cpu_height = cpu_panel_height(app.system_stats.cpus.len(), app.cpu_expanded)
            .min(area.height.saturating_sub(6));
        let rows =
            Layout::vertical([Constraint::Length(cpu_height), Constraint::Min(6)]).split(area);
        render_system_bars(f, app, rows[0]);
        render_secondary_panels(f, app, rows[1]);
    } else {
        let columns = overview_panel_rects(area, app.network_visible);
        render_system_bars(f, app, columns[0]);
        render_memory_panel(f, app, columns[1]);
        if app.network_visible {
            render_network_panel(f, app, columns[2]);
        }
    }
}

fn overview_panel_rects(area: Rect, network_visible: bool) -> Vec<Rect> {
    let constraints = if network_visible {
        vec![
            Constraint::Percentage(35),
            Constraint::Percentage(25),
            Constraint::Percentage(40),
        ]
    } else {
        vec![Constraint::Percentage(60), Constraint::Percentage(40)]
    };
    Layout::horizontal(constraints).split(area).to_vec()
}

fn render_secondary_panels(f: &mut Frame, app: &App, area: Rect) {
    if app.network_visible {
        let columns = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        render_memory_panel(f, app, columns[0]);
        render_network_panel(f, app, columns[1]);
    } else {
        render_memory_panel(f, app, area);
    }
}

pub fn render_system_bars(f: &mut Frame, app: &App, area: Rect) {
    let stats = &app.system_stats;
    let average = if stats.cpus.is_empty() {
        0.0
    } else {
        stats.cpus.iter().sum::<f32>() / stats.cpus.len() as f32
    };
    let title = Line::from(vec![
        Span::styled(" CPU ", Style::default().fg(ACCENT).bold()),
        Span::styled(
            if app.cpu_expanded {
                "[C: collapse]"
            } else {
                "[C: cores]"
            },
            Style::default().fg(MUTED),
        ),
    ]);
    let block = panel(title);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let summary_height = inner.height.min(2);
    let summary = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(summary_height.saturating_sub(1)),
        Constraint::Min(0),
    ])
    .split(inner);
    let label = Paragraph::new(Line::from(vec![
        Span::styled("CPU TOTAL ", Style::default().fg(Color::White).bold()),
        Span::styled(
            format!("{:>5.1}%", average),
            Style::default().fg(usage_color(average as f64)).bold(),
        ),
        Span::styled(
            format!("   {} logical cores", stats.cpus.len()),
            Style::default().fg(MUTED),
        ),
    ]));
    f.render_widget(label, summary[0]);
    if summary[1].height > 0 {
        let gauge = Gauge::default()
            .gauge_style(
                Style::default()
                    .fg(usage_color(average as f64))
                    .bg(Color::Black),
            )
            .ratio((average as f64 / 100.0).clamp(0.0, 1.0))
            .label("");
        f.render_widget(gauge, summary[1]);
    }

    if !app.cpu_expanded || inner.height <= 2 {
        return;
    }
    let core_area = Rect::new(
        inner.x,
        inner.y.saturating_add(2),
        inner.width,
        inner.height.saturating_sub(2),
    );
    let half = stats.cpus.len().div_ceil(2);
    let mut lines = Vec::new();
    for row in 0..half.min(core_area.height as usize) {
        let mut spans = core_meter(row, stats.cpus[row], core_area.width as usize / 2);
        let right = row + half;
        if right < stats.cpus.len() {
            spans.extend(core_meter(
                right,
                stats.cpus[right],
                core_area.width as usize / 2,
            ));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), core_area);
}

fn core_meter(index: usize, usage: f32, width: usize) -> Vec<Span<'static>> {
    let label = format!("Core {index:02} ");
    let percent = format!(" {:>5.1}%", usage);
    let bar_width = width.saturating_sub(label.len() + percent.len() + 1);
    let filled = ((usage / 100.0) * bar_width as f32).round() as usize;
    vec![
        Span::styled(label, Style::default().fg(ACCENT)),
        Span::styled(
            "━".repeat(filled.min(bar_width)),
            Style::default().fg(usage_color(usage as f64)),
        ),
        Span::styled(
            "─".repeat(bar_width.saturating_sub(filled)),
            Style::default().fg(MUTED),
        ),
        Span::styled(percent, Style::default().fg(Color::White)),
        Span::raw(" "),
    ]
}

fn render_memory_panel(f: &mut Frame, app: &App, area: Rect) {
    let stats = &app.system_stats;
    let mem_percent = if stats.mem_total == 0 {
        0.0
    } else {
        stats.mem_used as f64 / stats.mem_total as f64 * 100.0
    };
    let swap_percent = if stats.swap_total == 0 {
        0.0
    } else {
        stats.swap_used as f64 / stats.swap_total as f64 * 100.0
    };
    let block = panel(Line::from(Span::styled(
        " MEMORY / SYSTEM ",
        Style::default().fg(SECONDARY).bold(),
    )));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(if stats.swap_total > 0 { 2 } else { 0 }),
        Constraint::Min(1),
    ])
    .split(inner);
    let memory = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(usage_color(mem_percent))
                .bg(Color::Black),
        )
        .ratio((mem_percent / 100.0).clamp(0.0, 1.0))
        .label(format!(
            "MEM {:>4.1}%  {} / {}",
            mem_percent,
            format_bytes(stats.mem_used),
            format_bytes(stats.mem_total)
        ));
    f.render_widget(memory, rows[0]);
    if stats.swap_total > 0 {
        let swap = Gauge::default()
            .gauge_style(
                Style::default()
                    .fg(usage_color(swap_percent))
                    .bg(Color::Black),
            )
            .ratio((swap_percent / 100.0).clamp(0.0, 1.0))
            .label(format!(
                "SWP {:>4.1}%  {} / {}",
                swap_percent,
                format_bytes(stats.swap_used),
                format_bytes(stats.swap_total)
            ));
        f.render_widget(swap, rows[1]);
    }
    let system = Paragraph::new(vec![
        Line::from(format!(
            "LOAD  {:.2}  {:.2}  {:.2}",
            stats.load_average[0], stats.load_average[1], stats.load_average[2]
        )),
        Line::from(format!(
            "UP    {}   RUN {}/{}",
            format_uptime(stats.uptime),
            stats.running_processes,
            app.processes.len()
        )),
    ])
    .style(Style::default().fg(Color::Gray));
    f.render_widget(system, rows[2]);
}

pub fn render_network_panel(f: &mut Frame, app: &App, area: Rect) {
    let stats = &app.system_stats;
    let title = Line::from(vec![
        Span::styled(
            format!(" NETWORK · {} ", stats.network_interface),
            Style::default().fg(Color::Blue).bold(),
        ),
        Span::styled("[N: hide]", Style::default().fg(MUTED)),
    ]);
    let block = panel(title);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let graph_width = inner.width.saturating_sub(1) as usize;
    let rx: Vec<u64> = stats.network_rx_history.iter().copied().collect();
    let tx: Vec<u64> = stats.network_tx_history.iter().copied().collect();
    let status_color = if stats.network_status == "up" {
        Color::Green
    } else if stats.network_status == "down" {
        Color::Red
    } else {
        Color::Yellow
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled("IFACE ", Style::default().fg(MUTED)),
            Span::styled(
                &stats.network_interface,
                Style::default().fg(Color::White).bold(),
            ),
            Span::raw("  "),
            Span::styled(
                stats.network_status.to_uppercase(),
                Style::default().fg(status_color).bold(),
            ),
            Span::styled("  IP ", Style::default().fg(MUTED)),
            Span::styled(&stats.network_ip, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("MAC ", Style::default().fg(MUTED)),
            Span::styled(&stats.network_mac, Style::default().fg(Color::White)),
            Span::styled("   MTU ", Style::default().fg(MUTED)),
            Span::styled(
                stats.network_mtu.to_string(),
                Style::default().fg(Color::White),
            ),
        ]),
    ];
    let ipv6_rows = inner.height.saturating_sub(6) as usize;
    if ipv6_rows > 0 {
        if stats.network_ipv6.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("IPv6 ", Style::default().fg(MUTED)),
                Span::styled("—", Style::default().fg(Color::DarkGray)),
            ]));
        } else {
            lines.extend(
                visible_ipv6_addresses(&stats.network_ipv6, ipv6_rows)
                    .into_iter()
                    .map(|address| {
                        Line::from(vec![
                            Span::styled("IPv6 ", Style::default().fg(MUTED)),
                            Span::styled(address, Style::default().fg(Color::Gray)),
                        ])
                    }),
            );
        }
    }
    lines.extend([
        Line::from(vec![
            Span::styled("RX ", Style::default().fg(ACCENT).bold()),
            Span::styled(
                format_rate(stats.network_rx_rate),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("  Σ {}", format_bytes(stats.network_rx_total)),
                Style::default().fg(MUTED),
            ),
        ]),
        Line::styled(
            sparkline_text(&rx, graph_width),
            Style::default().fg(ACCENT),
        ),
        Line::from(vec![
            Span::styled("TX ", Style::default().fg(SECONDARY).bold()),
            Span::styled(
                format_rate(stats.network_tx_rate),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("  Σ {}", format_bytes(stats.network_tx_total)),
                Style::default().fg(MUTED),
            ),
        ]),
        Line::styled(
            sparkline_text(&tx, graph_width),
            Style::default().fg(SECONDARY),
        ),
    ]);
    f.render_widget(Paragraph::new(lines), inner);
}

pub fn render_process_table(f: &mut Frame, app: &App, area: Rect) {
    let visible_rows = visible_rows_for_area(area);
    let wide = area.width >= 110;
    let medium = area.width >= 78;
    let sort_indicator = |key: SortKey, label: &str| -> String {
        if app.sort_key == key {
            format!("{label} {}", if app.sort_asc { "▲" } else { "▼" })
        } else {
            label.to_string()
        }
    };
    let header_style = Style::default()
        .fg(Color::Black)
        .bg(ACCENT)
        .add_modifier(Modifier::BOLD);
    let header = if wide {
        Row::new([
            sort_indicator(SortKey::Pid, "PID"),
            sort_indicator(SortKey::Name, "PROCESS"),
            "STATE".to_string(),
            sort_indicator(SortKey::Cpu, "CPU%"),
            sort_indicator(SortKey::Mem, "MEM MiB"),
            "COMMAND".to_string(),
        ])
        .style(header_style)
    } else if medium {
        Row::new([
            sort_indicator(SortKey::Pid, "PID"),
            sort_indicator(SortKey::Name, "PROCESS"),
            "STATE".to_string(),
            sort_indicator(SortKey::Cpu, "CPU%"),
            sort_indicator(SortKey::Mem, "MEM MiB"),
        ])
        .style(header_style)
    } else {
        Row::new([
            sort_indicator(SortKey::Pid, "PID"),
            sort_indicator(SortKey::Name, "PROCESS"),
            sort_indicator(SortKey::Cpu, "CPU%"),
            sort_indicator(SortKey::Mem, "MEM"),
        ])
        .style(header_style)
    };

    let rows: Vec<Row> = app
        .filtered_processes()
        .skip(app.scroll_offset)
        .take(visible_rows)
        .map(|process| {
            let style = if app.selected_pid == Some(process.pid) {
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            let command = process
                .cmd
                .iter()
                .map(|argument| sanitize(argument))
                .collect::<Vec<_>>()
                .join(" ");
            let cells = if wide {
                vec![
                    process.pid.to_string(),
                    process.name.clone(),
                    process.state.clone(),
                    format!("{:.1}", process.cpu),
                    format!("{:.1}", process.mem_mib),
                    command,
                ]
            } else if medium {
                vec![
                    process.pid.to_string(),
                    process.name.clone(),
                    process.state.clone(),
                    format!("{:.1}", process.cpu),
                    format!("{:.1}", process.mem_mib),
                ]
            } else {
                vec![
                    process.pid.to_string(),
                    process.name.clone(),
                    format!("{:.1}", process.cpu),
                    format!("{:.0}", process.mem_mib),
                ]
            };
            Row::new(cells).style(style)
        })
        .collect();

    let widths: Vec<Constraint> = if wide {
        vec![
            Constraint::Length(8),
            Constraint::Percentage(24),
            Constraint::Length(9),
            Constraint::Length(8),
            Constraint::Length(11),
            Constraint::Min(24),
        ]
    } else if medium {
        vec![
            Constraint::Length(8),
            Constraint::Min(22),
            Constraint::Length(9),
            Constraint::Length(8),
            Constraint::Length(11),
        ]
    } else {
        vec![
            Constraint::Length(7),
            Constraint::Min(14),
            Constraint::Length(7),
            Constraint::Length(8),
        ]
    };
    let title = Line::from(vec![
        Span::styled(" PROCESSES ", Style::default().fg(ACCENT).bold()),
        Span::styled(
            format!("{}/{}", app.filtered_count(), app.processes.len()),
            Style::default().fg(Color::White),
        ),
    ]);
    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(1)
        .block(panel(title));
    f.render_widget(table, area);
}

pub fn render_command_panel(f: &mut Frame, app: &App, area: Rect) {
    let Some(pid) = app.expanded_cmd else {
        return;
    };
    let Some(process) = app.processes.iter().find(|process| process.pid == pid) else {
        return;
    };
    let command = process
        .cmd
        .iter()
        .map(|argument| sanitize(argument))
        .collect::<Vec<_>>()
        .join(" ");
    let lines = vec![
        Line::from(vec![
            Span::styled("STATE ", Style::default().fg(MUTED)),
            Span::styled(&process.state, Style::default().fg(Color::White)),
            Span::styled("   CPU ", Style::default().fg(MUTED)),
            Span::styled(format!("{:.1}%", process.cpu), Style::default().fg(ACCENT)),
            Span::styled("   MEM ", Style::default().fg(MUTED)),
            Span::styled(
                format!("{:.1} MiB", process.mem_mib),
                Style::default().fg(SECONDARY),
            ),
        ]),
        Line::from(Span::styled("COMMAND", Style::default().fg(MUTED))),
        Line::from(command),
    ];
    let title = Line::from(vec![
        Span::styled(" PROCESS DETAIL ", Style::default().fg(SECONDARY).bold()),
        Span::styled(
            format!("PID {pid} · {}", process.name),
            Style::default().fg(Color::White),
        ),
        Span::styled("  [Enter: close]", Style::default().fg(MUTED)),
    ]);
    f.render_widget(
        Paragraph::new(lines)
            .block(panel(title))
            .wrap(Wrap { trim: false }),
        area,
    );
}

pub fn render_help(f: &mut Frame, app: &App, area: Rect) {
    let filter_status = if app.filtering {
        format!("FILTER /{}█", app.filter)
    } else if app.filter.is_empty() {
        "FILTER off".to_string()
    } else {
        format!("FILTER /{}", app.filter)
    };
    let first = Line::from(vec![
        key("1-4"),
        Span::raw(":Sort  "),
        key("↑↓/JK"),
        Span::raw(":Move  "),
        key("G/Home/End/Pg"),
        Span::raw(":Jump  "),
        key("Enter"),
        Span::raw(":Detail  "),
        key("Space"),
        Span::raw(":Pause"),
    ]);
    let second = Line::from(vec![
        key("C"),
        Span::raw(":Cores  "),
        key("N"),
        Span::raw(":Net  "),
        key("/"),
        Span::raw(":Filter  "),
        key("F"),
        Span::raw(":Filter  "),
        key("Esc/Q"),
        Span::raw(":Clear/Quit  "),
        Span::styled(filter_status, Style::default().fg(SECONDARY).bold()),
    ]);
    let lines = if area.height >= 4 {
        vec![first, second]
    } else {
        vec![second]
    };
    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(MUTED)),
            )
            .style(Style::default().fg(Color::Gray)),
        area,
    );
}

fn key(label: &'static str) -> Span<'static> {
    Span::styled(label, Style::default().fg(Color::Yellow).bold())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn rendered_text(width: u16, height: u16, draw: impl FnOnce(&mut Frame)) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(draw).unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn dashboard_uses_wide_layout_only_when_there_is_room_for_columns() {
        assert_eq!(
            dashboard_mode(Rect::new(0, 0, 120, 40)),
            DashboardMode::Wide
        );
        assert_eq!(
            dashboard_mode(Rect::new(0, 0, 99, 40)),
            DashboardMode::Narrow
        );
        assert_eq!(
            dashboard_mode(Rect::new(0, 0, 120, 27)),
            DashboardMode::Narrow
        );
    }

    #[test]
    fn expanded_cpu_panel_reserves_more_rows_for_per_core_meters() {
        assert_eq!(cpu_panel_height(16, false), 5);
        assert_eq!(cpu_panel_height(16, true), 12);
        assert_eq!(cpu_panel_height(1, true), 5);
    }

    #[test]
    fn sparkline_levels_scale_against_the_highest_sample() {
        assert_eq!(sparkline_levels(&[0, 5, 10]), vec![0, 4, 8]);
        assert_eq!(sparkline_levels(&[0, 0]), vec![0, 0]);
    }

    #[test]
    fn sparkline_keeps_zero_empty_but_marks_every_nonzero_sample() {
        assert_eq!(sparkline_levels(&[0, 1, 1_000]), vec![0, 1, 8]);
        assert_eq!(sparkline_text(&[0, 1, 1_000], 3), " ▁█");
    }

    #[test]
    fn cpu_panel_switches_between_summary_and_per_core_detail() {
        let mut app = App::new();
        app.system_stats.cpus = vec![25.0, 75.0];

        let summary = rendered_text(80, 8, |frame| render_system_bars(frame, &app, frame.area()));
        assert!(summary.contains("CPU TOTAL"));
        assert!(!summary.contains("Core 00"));

        app.cpu_expanded = true;
        let expanded = rendered_text(80, 8, |frame| render_system_bars(frame, &app, frame.area()));
        assert!(expanded.contains("Core 00"));
        assert!(expanded.contains("Core 01"));
    }

    #[test]
    fn network_panel_shows_rates_totals_and_visual_history() {
        let mut app = App::new();
        app.system_stats.set_network_interface(
            "en0",
            "up",
            "192.0.2.10/24",
            &["fe80::1/64".into()],
            "00:11:22:33:44:55",
            1500,
        );
        app.system_stats.network_rx_rate = 1_024;
        app.system_stats.network_tx_rate = 2_048;
        app.system_stats.network_rx_total = 1_048_576;
        app.system_stats.network_tx_total = 2_097_152;
        app.system_stats.record_network_sample(1_024, 2_048);

        let output = rendered_text(80, 8, |frame| {
            render_network_panel(frame, &app, frame.area())
        });

        assert!(output.contains("NETWORK"));
        assert!(output.contains("RX"));
        assert!(output.contains("TX"));
        assert!(output.contains("en0"));
        assert!(output.contains("UP"));
        assert!(output.contains("192.0.2.10/24"));
        assert!(output.contains("00:11:22:33:44:55"));
        assert!(output.contains("MTU 1500"));
        assert!(output.contains("1.00 MiB"));
        assert!(output.contains('█'));
        assert!(!output.contains("IPv6"));
    }

    #[test]
    fn taller_network_panel_shows_global_and_link_local_ipv6_secondarily() {
        let mut app = App::new();
        app.system_stats.set_network_interface(
            "en0",
            "up",
            "192.0.2.10/24",
            &[
                "2001:db8::10/64".into(),
                "2001:db8::20/64".into(),
                "fe80::1/64".into(),
            ],
            "00:11:22:33:44:55",
            1500,
        );

        let output = rendered_text(80, 10, |frame| {
            render_network_panel(frame, &app, frame.area())
        });

        assert!(output.contains("IPv6 2001:db8::10/64"));
        assert!(output.contains("IPv6 fe80::1/64"));
        assert!(!output.contains("2001:db8::20/64"));
        assert!(output.contains("IP 192.0.2.10/24"));
    }

    #[test]
    fn wide_dashboard_gives_network_history_at_least_forty_percent() {
        let panels = overview_panel_rects(Rect::new(0, 0, 120, 12), true);

        assert_eq!(panels.len(), 3);
        assert!(panels[2].width >= 48);
        assert_eq!(panels[2].height, 12);
    }

    #[test]
    fn wide_process_table_has_state_and_command_columns() {
        let app = App::new();
        let output = rendered_text(140, 10, |frame| {
            render_process_table(frame, &app, frame.area())
        });

        assert!(output.contains("STATE"));
        assert!(output.contains("COMMAND"));
    }

    #[test]
    fn help_discovers_panel_and_filter_shortcuts() {
        let app = App::new();
        let output = rendered_text(140, 6, |frame| render_help(frame, &app, frame.area()));

        assert!(output.contains("C:Cores"));
        assert!(output.contains("N:Net"));
        assert!(output.contains("/:Filter"));
        assert!(output.contains("F:Filter"));
    }
}
