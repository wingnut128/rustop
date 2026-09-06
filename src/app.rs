use std::{cmp::Ordering, collections::VecDeque, time::Instant};
use sysinfo::{InterfaceOperationalState, IpNetwork, Networks, System};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Cpu,
    Mem,
    Pid,
    Name,
}

impl SortKey {
    /// Returns the natural default sort direction for this key.
    /// CPU and MEM default to descending (highest first).
    /// PID and Name default to ascending.
    pub fn default_ascending(self) -> bool {
        matches!(self, SortKey::Pid | SortKey::Name)
    }
}

pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub name_lower: String,
    pub cpu: f32,
    pub mem_mib: f64,
    pub cmd: Vec<String>,
    pub state: String,
}

pub struct SystemStats {
    pub cpus: Vec<f32>,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub network_rx_history: VecDeque<u64>,
    pub network_tx_history: VecDeque<u64>,
    pub network_rx_rate: u64,
    pub network_tx_rate: u64,
    pub network_rx_total: u64,
    pub network_tx_total: u64,
    pub network_interface: String,
    pub network_status: String,
    pub network_ip: String,
    pub network_ipv6: Vec<String>,
    pub network_mac: String,
    pub network_mtu: u64,
    pub load_average: [f64; 3],
    pub uptime: u64,
    pub running_processes: usize,
}

impl SystemStats {
    fn new() -> Self {
        Self {
            cpus: Vec::new(),
            mem_used: 0,
            mem_total: 0,
            swap_used: 0,
            swap_total: 0,
            network_rx_history: VecDeque::with_capacity(60),
            network_tx_history: VecDeque::with_capacity(60),
            network_rx_rate: 0,
            network_tx_rate: 0,
            network_rx_total: 0,
            network_tx_total: 0,
            network_interface: "detecting".to_string(),
            network_status: "unknown".to_string(),
            network_ip: "—".to_string(),
            network_ipv6: Vec::new(),
            network_mac: "—".to_string(),
            network_mtu: 0,
            load_average: [0.0; 3],
            uptime: 0,
            running_processes: 0,
        }
    }

    pub fn record_network_sample(&mut self, received: u64, transmitted: u64) {
        const HISTORY_LENGTH: usize = 60;
        if self.network_rx_history.len() == HISTORY_LENGTH {
            self.network_rx_history.pop_front();
            self.network_tx_history.pop_front();
        }
        self.network_rx_history.push_back(received);
        self.network_tx_history.push_back(transmitted);
        self.network_rx_rate = received;
        self.network_tx_rate = transmitted;
    }

    pub fn set_network_interface(
        &mut self,
        name: &str,
        status: &str,
        ip: &str,
        ipv6: &[String],
        mac: &str,
        mtu: u64,
    ) {
        if self.network_interface != name {
            self.network_rx_history.clear();
            self.network_tx_history.clear();
            self.network_rx_rate = 0;
            self.network_tx_rate = 0;
            self.network_rx_total = 0;
            self.network_tx_total = 0;
        }
        name.clone_into(&mut self.network_interface);
        status.clone_into(&mut self.network_status);
        ip.clone_into(&mut self.network_ip);
        self.network_ipv6.clear();
        self.network_ipv6.extend_from_slice(ipv6);
        mac.clone_into(&mut self.network_mac);
        self.network_mtu = mtu;
    }
}

fn select_network_interface<'a>(
    interfaces: &'a [(&'a str, u64, u64, bool, bool, bool)],
) -> Option<&'a str> {
    let has_non_loopback = interfaces.iter().any(|interface| !interface.3);
    interfaces
        .iter()
        .filter(|interface| !has_non_loopback || !interface.3)
        .max_by(|left, right| {
            let left_total = left.1.saturating_add(left.2);
            let right_total = right.1.saturating_add(right.2);
            left_total
                .cmp(&right_total)
                .then_with(|| left.5.cmp(&right.5))
                .then_with(|| left.4.cmp(&right.4))
                .then_with(|| right.0.cmp(left.0))
        })
        .map(|interface| interface.0)
}

fn has_preferred_network_address(addresses: &[IpNetwork]) -> bool {
    addresses
        .iter()
        .any(|network| network.addr.is_ipv4() && !network.addr.is_loopback())
}

fn interface_addresses(addresses: &[IpNetwork]) -> (String, Vec<String>) {
    let ipv4 = addresses
        .iter()
        .find(|network| network.addr.is_ipv4() && !network.addr.is_loopback())
        .or_else(|| addresses.iter().find(|network| network.addr.is_ipv4()))
        .map(ToString::to_string)
        .unwrap_or_else(|| "—".to_string());
    let mut ipv6: Vec<_> = addresses
        .iter()
        .filter(|network| network.addr.is_ipv6())
        .map(ToString::to_string)
        .collect();
    ipv6.sort();
    (ipv4, ipv6)
}

/// Replace characters that can alter terminal layout or visual reading order.
pub fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_control()
                || matches!(
                    c,
                    '\u{200B}'..='\u{200F}'
                        | '\u{202A}'..='\u{202E}'
                        | '\u{2060}'..='\u{206F}'
                        | '\u{FEFF}'
                )
            {
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect()
}

pub struct App {
    system: System,
    networks: Networks,
    last_network_refresh: Instant,
    pub processes: Vec<ProcessInfo>,
    pub system_stats: SystemStats,
    pub sort_key: SortKey,
    pub sort_asc: bool,
    pub selected_pid: Option<u32>,
    pub expanded_cmd: Option<u32>,
    pub scroll_offset: usize,
    pub visible_rows: usize,
    pub filter: String,
    pub filtering: bool,
    pub paused: bool,
    pub cpu_expanded: bool,
    pub network_visible: bool,
}

impl App {
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        Self {
            system,
            networks: Networks::new_with_refreshed_list(),
            last_network_refresh: Instant::now(),
            processes: Vec::new(),
            system_stats: SystemStats::new(),
            sort_key: SortKey::Cpu,
            sort_asc: SortKey::Cpu.default_ascending(),
            selected_pid: None,
            expanded_cmd: None,
            scroll_offset: 0,
            visible_rows: 20,
            filter: String::new(),
            filtering: false,
            paused: false,
            cpu_expanded: false,
            network_visible: true,
        }
    }

    pub fn refresh_processes(&mut self) {
        self.system.refresh_all();
        self.networks.refresh(true);

        // Update system-level stats
        self.system_stats.cpus.clear();
        self.system_stats
            .cpus
            .extend(self.system.cpus().iter().map(|cpu| cpu.cpu_usage()));
        self.system_stats.mem_used = self.system.used_memory();
        self.system_stats.mem_total = self.system.total_memory();
        self.system_stats.swap_used = self.system.used_swap();
        self.system_stats.swap_total = self.system.total_swap();
        let load = System::load_average();
        self.system_stats.load_average = [load.one, load.five, load.fifteen];
        self.system_stats.uptime = System::uptime();

        let elapsed = self.last_network_refresh.elapsed().as_secs_f64().max(0.001);
        self.last_network_refresh = Instant::now();
        let candidates: Vec<_> = self
            .networks
            .iter()
            .map(|(name, network)| {
                let addresses = network.ip_networks();
                let loopback = name.starts_with("lo")
                    || (!addresses.is_empty()
                        && addresses.iter().all(|network| network.addr.is_loopback()));
                let is_up = network.operational_state() == InterfaceOperationalState::Up;
                let has_address = has_preferred_network_address(addresses);
                (
                    name.as_str(),
                    network.total_received(),
                    network.total_transmitted(),
                    loopback,
                    is_up,
                    has_address,
                )
            })
            .collect();
        if let Some(name) = select_network_interface(&candidates)
            && let Some(network) = self.networks.get(name)
        {
            let (ip, ipv6) = interface_addresses(network.ip_networks());
            let mac = network.mac_address();
            let mac = if mac.is_unspecified() {
                "—".to_string()
            } else {
                mac.to_string()
            };
            self.system_stats.set_network_interface(
                name,
                &network.operational_state().to_string(),
                &ip,
                &ipv6,
                &mac,
                network.mtu(),
            );
            self.system_stats.network_rx_total = network.total_received();
            self.system_stats.network_tx_total = network.total_transmitted();
            self.system_stats.record_network_sample(
                (network.received() as f64 / elapsed) as u64,
                (network.transmitted() as f64 / elapsed) as u64,
            );
        }

        self.processes.clear();
        self.processes
            .extend(self.system.processes().iter().map(|(pid, process)| {
                let cmd: Vec<String> = process
                    .cmd()
                    .iter()
                    .map(|s| s.to_string_lossy().into_owned())
                    .collect();
                let name = sanitize(&process.name().to_string_lossy());
                let name_lower = name.to_lowercase();
                let mem_mib = process.memory() as f64 / 1024.0 / 1024.0;
                ProcessInfo {
                    pid: pid.as_u32(),
                    name,
                    name_lower,
                    cpu: process.cpu_usage(),
                    mem_mib,
                    cmd,
                    state: format!("{:?}", process.status()),
                }
            }));

        self.system_stats.running_processes = self
            .processes
            .iter()
            .filter(|process| process.state == "Run")
            .count();

        self.sort_processes();

        // Clear selected_pid if the process died
        if let Some(selected) = self.selected_pid
            && !self.processes.iter().any(|p| p.pid == selected)
        {
            self.selected_pid = None;
        }

        // Clear expanded_cmd if the process died
        if let Some(expanded) = self.expanded_cmd
            && !self.processes.iter().any(|p| p.pid == expanded)
        {
            self.expanded_cmd = None;
        }

        // Clamp scroll_offset so it never points past the list
        self.clamp_selection_and_scroll();
    }

    fn sort_processes(&mut self) {
        self.processes.sort_by(|a, b| {
            let ord = match self.sort_key {
                SortKey::Cpu => a.cpu.partial_cmp(&b.cpu).unwrap_or(Ordering::Equal),
                SortKey::Mem => a.mem_mib.partial_cmp(&b.mem_mib).unwrap_or(Ordering::Equal),
                SortKey::Pid => a.pid.cmp(&b.pid),
                SortKey::Name => a.name_lower.cmp(&b.name_lower),
            };
            if self.sort_asc { ord } else { ord.reverse() }
        });
    }

    pub fn filtered_processes(&self) -> impl Iterator<Item = &ProcessInfo> {
        let filter = self.filter.to_lowercase();
        self.processes
            .iter()
            .filter(move |process| filter.is_empty() || process.name_lower.contains(&filter))
    }

    pub fn filtered_count(&self) -> usize {
        self.filtered_processes().count()
    }

    fn clamp_selection_and_scroll(&mut self) {
        let filtered_pids: Vec<u32> = self
            .filtered_processes()
            .map(|process| process.pid)
            .collect();
        if filtered_pids.is_empty() {
            self.scroll_offset = 0;
            self.selected_pid = None;
        } else {
            if self
                .selected_pid
                .is_some_and(|pid| !filtered_pids.contains(&pid))
            {
                self.selected_pid = None;
            }
            let max_offset = filtered_pids.len().saturating_sub(1);
            self.scroll_offset = self.scroll_offset.min(max_offset);
        }
    }

    pub fn toggle_sort(&mut self, key: SortKey) {
        if self.sort_key == key {
            self.sort_asc = !self.sort_asc;
        } else {
            self.sort_key = key;
            self.sort_asc = key.default_ascending();
        }
        self.clamp_selection_and_scroll();
    }

    pub fn move_selection(&mut self, delta: isize) {
        let filtered_pids: Vec<u32> = self
            .filtered_processes()
            .map(|process| process.pid)
            .collect();
        if filtered_pids.is_empty() {
            return;
        }

        let max_idx = filtered_pids.len() - 1;

        let current_idx = self
            .selected_pid
            .and_then(|pid| filtered_pids.iter().position(|&candidate| candidate == pid));

        let new_idx = match current_idx {
            Some(idx) => (idx as isize + delta).clamp(0, max_idx as isize) as usize,
            // First movement just selects the first visible item
            None => self.scroll_offset.min(max_idx),
        };
        self.select_filtered_index(new_idx, &filtered_pids);
    }

    pub fn move_to_edge(&mut self, last: bool) {
        let filtered_pids: Vec<u32> = self
            .filtered_processes()
            .map(|process| process.pid)
            .collect();
        if !filtered_pids.is_empty() {
            let index = if last { filtered_pids.len() - 1 } else { 0 };
            self.select_filtered_index(index, &filtered_pids);
        }
    }

    fn select_filtered_index(&mut self, new_idx: usize, filtered_pids: &[u32]) {
        self.selected_pid = Some(filtered_pids[new_idx]);

        // Adjust scroll to keep selection visible
        if new_idx < self.scroll_offset {
            self.scroll_offset = new_idx;
        } else if self.visible_rows > 0 && new_idx >= self.scroll_offset + self.visible_rows {
            self.scroll_offset = new_idx.saturating_sub(self.visible_rows - 1);
        }
    }

    pub fn toggle_expand(&mut self) {
        if self.selected_pid == self.expanded_cmd {
            self.expanded_cmd = None;
        } else {
            self.expanded_cmd = self.selected_pid;
        }
    }

    pub fn begin_filter(&mut self) {
        self.filtering = true;
    }

    pub fn end_filter(&mut self) {
        self.filtering = false;
    }

    pub fn push_filter_char(&mut self, character: char) {
        self.filter.push(character);
        self.clamp_selection_and_scroll();
    }

    pub fn pop_filter_char(&mut self) {
        self.filter.pop();
        self.clamp_selection_and_scroll();
    }

    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.filtering = false;
        self.clamp_selection_and_scroll();
    }

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    pub fn toggle_cpu_expanded(&mut self) {
        self.cpu_expanded = !self.cpu_expanded;
    }

    pub fn toggle_network_visible(&mut self) {
        self.network_visible = !self.network_visible;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_process(pid: u32, name: &str, cpu: f32, mem_mib: f64) -> ProcessInfo {
        ProcessInfo {
            pid,
            name: name.to_string(),
            name_lower: name.to_lowercase(),
            cpu,
            mem_mib,
            cmd: vec![format!("/usr/bin/{}", name)],
            state: "Run".to_string(),
        }
    }

    fn make_app_with(procs: Vec<ProcessInfo>) -> App {
        App {
            system: System::new(),
            networks: Networks::new(),
            last_network_refresh: Instant::now(),
            processes: procs,
            system_stats: SystemStats::new(),
            sort_key: SortKey::Cpu,
            sort_asc: false,
            selected_pid: None,
            expanded_cmd: None,
            scroll_offset: 0,
            visible_rows: 5,
            filter: String::new(),
            filtering: false,
            paused: false,
            cpu_expanded: false,
            network_visible: true,
        }
    }

    // ── sanitize ──────────────────────────────────────────────

    #[test]
    fn sanitize_strips_escape_sequences() {
        assert_eq!(sanitize("hello\x1b[31mworld"), "hello\u{FFFD}[31mworld");
    }

    #[test]
    fn sanitize_preserves_normal_text() {
        assert_eq!(sanitize("normal text"), "normal text");
    }

    #[test]
    fn sanitize_replaces_layout_and_direction_controls() {
        assert_eq!(sanitize("tab\there"), "tab\u{FFFD}here");
        assert_eq!(sanitize("newline\nhere"), "newline\u{FFFD}here");
        assert_eq!(sanitize("abc\u{202E}def\u{200B}"), "abc\u{FFFD}def\u{FFFD}");
    }

    #[test]
    fn sanitize_strips_null_bytes() {
        assert_eq!(sanitize("ab\x00cd"), "ab\u{FFFD}cd");
    }

    // ── SortKey ───────────────────────────────────────────────

    #[test]
    fn sort_key_default_ascending() {
        assert!(SortKey::Name.default_ascending());
        assert!(SortKey::Pid.default_ascending());
        assert!(!SortKey::Cpu.default_ascending());
        assert!(!SortKey::Mem.default_ascending());
    }

    #[test]
    fn sort_key_is_copy() {
        let k = SortKey::Cpu;
        let k2 = k; // Copy
        assert_eq!(k, k2);
    }

    // ── toggle_sort ───────────────────────────────────────────

    #[test]
    fn toggle_sort_same_key_flips_direction() {
        let mut app = make_app_with(vec![]);
        app.sort_key = SortKey::Cpu;
        app.sort_asc = false;

        app.toggle_sort(SortKey::Cpu);
        assert!(app.sort_asc);

        app.toggle_sort(SortKey::Cpu);
        assert!(!app.sort_asc);
    }

    #[test]
    fn toggle_sort_different_key_uses_natural_default() {
        let mut app = make_app_with(vec![]);
        app.sort_key = SortKey::Cpu;
        app.sort_asc = false;

        // Name defaults to ascending
        app.toggle_sort(SortKey::Name);
        assert_eq!(app.sort_key, SortKey::Name);
        assert!(app.sort_asc);

        // Mem defaults to descending
        app.toggle_sort(SortKey::Mem);
        assert_eq!(app.sort_key, SortKey::Mem);
        assert!(!app.sort_asc);

        // PID defaults to ascending
        app.toggle_sort(SortKey::Pid);
        assert_eq!(app.sort_key, SortKey::Pid);
        assert!(app.sort_asc);
    }

    // ── move_selection ────────────────────────────────────────

    #[test]
    fn move_selection_empty_is_noop() {
        let mut app = make_app_with(vec![]);
        app.move_selection(1);
        assert_eq!(app.selected_pid, None);
    }

    #[test]
    fn move_selection_first_move_selects_index_zero() {
        let procs = vec![
            make_process(10, "alpha", 10.0, 100.0),
            make_process(20, "beta", 20.0, 200.0),
        ];
        let mut app = make_app_with(procs);

        app.move_selection(1);
        assert_eq!(app.selected_pid, Some(10));
    }

    #[test]
    fn move_selection_navigates_down() {
        let procs = vec![
            make_process(1, "a", 10.0, 100.0),
            make_process(2, "b", 20.0, 200.0),
            make_process(3, "c", 30.0, 300.0),
        ];
        let mut app = make_app_with(procs);
        app.selected_pid = Some(1);

        app.move_selection(1);
        assert_eq!(app.selected_pid, Some(2));

        app.move_selection(1);
        assert_eq!(app.selected_pid, Some(3));
    }

    #[test]
    fn move_selection_clamps_at_bottom() {
        let procs = vec![
            make_process(1, "a", 10.0, 100.0),
            make_process(2, "b", 20.0, 200.0),
        ];
        let mut app = make_app_with(procs);
        app.selected_pid = Some(2);

        app.move_selection(1);
        assert_eq!(app.selected_pid, Some(2));
    }

    #[test]
    fn move_selection_clamps_at_top() {
        let procs = vec![
            make_process(1, "a", 10.0, 100.0),
            make_process(2, "b", 20.0, 200.0),
        ];
        let mut app = make_app_with(procs);
        app.selected_pid = Some(1);

        app.move_selection(-1);
        assert_eq!(app.selected_pid, Some(1));
    }

    #[test]
    fn move_selection_scrolls_down_when_past_visible() {
        let procs: Vec<ProcessInfo> = (0..10)
            .map(|i| make_process(i, &format!("p{}", i), i as f32, i as f64))
            .collect();
        let mut app = make_app_with(procs);
        app.visible_rows = 3;
        app.selected_pid = Some(0);

        // Move to index 1, 2 — still visible
        app.move_selection(1);
        app.move_selection(1);
        assert_eq!(app.scroll_offset, 0);

        // Move to index 3 — should trigger scroll
        app.move_selection(1);
        assert!(app.scroll_offset > 0);
    }

    #[test]
    fn move_selection_scrolls_up_when_above_visible() {
        let procs: Vec<ProcessInfo> = (0..10)
            .map(|i| make_process(i, &format!("p{}", i), i as f32, i as f64))
            .collect();
        let mut app = make_app_with(procs);
        app.visible_rows = 3;
        app.scroll_offset = 5;
        app.selected_pid = Some(5);

        // Move up past the scroll offset
        app.move_selection(-1);
        assert_eq!(app.selected_pid, Some(4));
        assert_eq!(app.scroll_offset, 4);
    }

    // ── toggle_expand ─────────────────────────────────────────

    #[test]
    fn toggle_expand_expands_selected() {
        let mut app = make_app_with(vec![]);
        app.selected_pid = Some(42);

        app.toggle_expand();
        assert_eq!(app.expanded_cmd, Some(42));
    }

    #[test]
    fn toggle_expand_collapses_when_same() {
        let mut app = make_app_with(vec![]);
        app.selected_pid = Some(42);
        app.expanded_cmd = Some(42);

        app.toggle_expand();
        assert_eq!(app.expanded_cmd, None);
    }

    #[test]
    fn toggle_expand_switches_to_new_pid() {
        let mut app = make_app_with(vec![]);
        app.selected_pid = Some(42);
        app.expanded_cmd = Some(99);

        app.toggle_expand();
        assert_eq!(app.expanded_cmd, Some(42));
    }

    // ── filtering and scroll clamping ─────────────────────────

    #[test]
    fn clamp_scroll_offset_resets_on_empty() {
        let mut app = make_app_with(vec![]);
        app.scroll_offset = 50;

        app.clamp_selection_and_scroll();
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn clamp_scroll_offset_clamps_to_last_item() {
        let procs = vec![make_process(1, "a", 1.0, 1.0)];
        let mut app = make_app_with(procs);
        app.scroll_offset = 50;

        app.clamp_selection_and_scroll();
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn clamp_scroll_offset_preserves_valid_offset() {
        let procs: Vec<ProcessInfo> = (0..10)
            .map(|i| make_process(i, &format!("p{}", i), i as f32, i as f64))
            .collect();
        let mut app = make_app_with(procs);
        app.scroll_offset = 5;

        app.clamp_selection_and_scroll();
        assert_eq!(app.scroll_offset, 5);
    }

    #[test]
    fn filter_is_case_insensitive_and_clears_invalid_selection() {
        let mut app = make_app_with(vec![
            make_process(1, "Alpha", 1.0, 1.0),
            make_process(2, "beta", 1.0, 1.0),
        ]);
        app.selected_pid = Some(2);
        app.push_filter_char('A');
        app.push_filter_char('L');

        let pids: Vec<u32> = app
            .filtered_processes()
            .map(|process| process.pid)
            .collect();
        assert_eq!(pids, vec![1]);
        assert_eq!(app.selected_pid, None);
    }

    #[test]
    fn edge_navigation_selects_first_and_last_filtered_process() {
        let mut app = make_app_with(vec![
            make_process(1, "alpha", 1.0, 1.0),
            make_process(2, "beta", 1.0, 1.0),
            make_process(3, "alpine", 1.0, 1.0),
        ]);
        app.filter = "al".to_string();

        app.move_to_edge(true);
        assert_eq!(app.selected_pid, Some(3));
        app.move_to_edge(false);
        assert_eq!(app.selected_pid, Some(1));
    }

    #[test]
    fn pause_state_toggles() {
        let mut app = make_app_with(vec![]);
        assert!(!app.paused);
        app.toggle_pause();
        assert!(app.paused);
        app.toggle_pause();
        assert!(!app.paused);
    }

    #[test]
    fn dashboard_panels_default_to_summary_cpu_and_visible_network() {
        let app = make_app_with(vec![]);

        assert!(!app.cpu_expanded);
        assert!(app.network_visible);
    }

    #[test]
    fn dashboard_panel_toggles_are_independent() {
        let mut app = make_app_with(vec![]);

        app.toggle_cpu_expanded();
        assert!(app.cpu_expanded);
        assert!(app.network_visible);

        app.toggle_network_visible();
        assert!(app.cpu_expanded);
        assert!(!app.network_visible);
    }

    #[test]
    fn network_history_keeps_the_most_recent_sixty_samples() {
        let mut stats = SystemStats::new();

        for sample in 0..65 {
            stats.record_network_sample(sample, sample * 2);
        }

        assert_eq!(stats.network_rx_history.len(), 60);
        assert_eq!(stats.network_tx_history.len(), 60);
        assert_eq!(stats.network_rx_history.front(), Some(&5));
        assert_eq!(stats.network_rx_history.back(), Some(&64));
        assert_eq!(stats.network_tx_history.back(), Some(&128));
    }

    #[test]
    fn network_sample_updates_current_rates() {
        let mut stats = SystemStats::new();

        stats.record_network_sample(1_024, 2_048);

        assert_eq!(stats.network_rx_rate, 1_024);
        assert_eq!(stats.network_tx_rate, 2_048);
    }

    #[test]
    fn busiest_non_loopback_interface_wins_even_when_loopback_is_busier() {
        let interfaces = [
            ("lo0", 50_000, 50_000, true, true, true),
            ("en0", 2_000, 1_000, false, true, true),
            ("en1", 4_000, 3_000, false, false, false),
        ];

        assert_eq!(select_network_interface(&interfaces), Some("en1"));
    }

    #[test]
    fn network_interface_fallback_is_alphabetical_when_traffic_is_tied() {
        let interfaces = [
            ("utun1", 0, 0, false, true, true),
            ("en1", 0, 0, false, true, true),
            ("en0", 0, 0, false, true, true),
        ];

        assert_eq!(select_network_interface(&interfaces), Some("en0"));
    }

    #[test]
    fn zero_traffic_fallback_prefers_an_up_addressed_interface() {
        let interfaces = [
            ("anpi0", 0, 0, false, true, false),
            ("en0", 0, 0, false, true, true),
            ("en1", 0, 0, false, false, true),
        ];

        assert_eq!(select_network_interface(&interfaces), Some("en0"));
    }

    #[test]
    fn preferred_interface_address_requires_non_loopback_ipv4() {
        let link_local = ["fe80::1/64".parse().unwrap()];
        let ipv4 = ["192.0.2.10/24".parse().unwrap()];

        assert!(!has_preferred_network_address(&link_local));
        assert!(has_preferred_network_address(&ipv4));
    }

    #[test]
    fn interface_addresses_keep_ipv4_primary_and_include_all_ipv6() {
        let addresses = [
            "fe80::1/64".parse().unwrap(),
            "2001:db8::10/64".parse().unwrap(),
            "192.0.2.10/24".parse().unwrap(),
        ];

        let (ipv4, ipv6) = interface_addresses(&addresses);

        assert_eq!(ipv4, "192.0.2.10/24");
        assert_eq!(ipv6, vec!["2001:db8::10/64", "fe80::1/64"]);
    }

    #[test]
    fn switching_monitored_interface_starts_a_fresh_history() {
        let mut stats = SystemStats::new();
        stats.set_network_interface("en0", "up", "192.0.2.10/24", &[], "00:11:22:33:44:55", 1500);
        stats.record_network_sample(10, 20);

        stats.set_network_interface(
            "en1",
            "down",
            "198.51.100.4/24",
            &[],
            "66:77:88:99:aa:bb",
            9000,
        );
        stats.record_network_sample(30, 40);

        assert_eq!(stats.network_interface, "en1");
        assert_eq!(stats.network_status, "down");
        assert_eq!(stats.network_ip, "198.51.100.4/24");
        assert_eq!(stats.network_mac, "66:77:88:99:aa:bb");
        assert_eq!(stats.network_mtu, 9000);
        assert_eq!(
            stats.network_rx_history.iter().copied().collect::<Vec<_>>(),
            vec![30]
        );
        assert_eq!(
            stats.network_tx_history.iter().copied().collect::<Vec<_>>(),
            vec![40]
        );
    }

    // ── sort_processes ────────────────────────────────────────

    #[test]
    fn sort_by_cpu_descending() {
        let procs = vec![
            make_process(1, "low", 1.0, 1.0),
            make_process(2, "high", 90.0, 1.0),
            make_process(3, "mid", 50.0, 1.0),
        ];
        let mut app = make_app_with(procs);
        app.sort_key = SortKey::Cpu;
        app.sort_asc = false;

        app.sort_processes();

        let pids: Vec<u32> = app.processes.iter().map(|p| p.pid).collect();
        assert_eq!(pids, vec![2, 3, 1]);
    }

    #[test]
    fn sort_by_name_ascending() {
        let procs = vec![
            make_process(1, "Charlie", 1.0, 1.0),
            make_process(2, "alpha", 1.0, 1.0),
            make_process(3, "Bravo", 1.0, 1.0),
        ];
        let mut app = make_app_with(procs);
        app.sort_key = SortKey::Name;
        app.sort_asc = true;

        app.sort_processes();

        let names: Vec<&str> = app.processes.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "Bravo", "Charlie"]);
    }

    #[test]
    fn sort_by_pid_ascending() {
        let procs = vec![
            make_process(30, "c", 1.0, 1.0),
            make_process(10, "a", 1.0, 1.0),
            make_process(20, "b", 1.0, 1.0),
        ];
        let mut app = make_app_with(procs);
        app.sort_key = SortKey::Pid;
        app.sort_asc = true;

        app.sort_processes();

        let pids: Vec<u32> = app.processes.iter().map(|p| p.pid).collect();
        assert_eq!(pids, vec![10, 20, 30]);
    }

    #[test]
    fn sort_by_mem_descending() {
        let procs = vec![
            make_process(1, "low", 1.0, 10.0),
            make_process(2, "high", 1.0, 900.0),
            make_process(3, "mid", 1.0, 500.0),
        ];
        let mut app = make_app_with(procs);
        app.sort_key = SortKey::Mem;
        app.sort_asc = false;

        app.sort_processes();

        let pids: Vec<u32> = app.processes.iter().map(|p| p.pid).collect();
        assert_eq!(pids, vec![2, 3, 1]);
    }
}
