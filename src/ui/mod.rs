// SPDX-License-Identifier: Apache-2.0

pub mod colors;
pub mod constellation;
pub mod fabric;
pub mod fleet;
pub mod memory;
pub mod overview;

use std::collections::BTreeMap;
use std::io;
use std::time::{Duration, Instant};

use chrono::Utc;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::{Frame, Terminal};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Gauge, Paragraph, Tabs};
use unicode_width::UnicodeWidthStr;

use crate::cli::ViewMode;
use crate::collector::Collector;
use crate::model::{DeviceSnapshot, Snapshot};
use crate::ui::colors::*;

#[derive(Debug, Clone, Default)]
pub struct AdaptiveBaseline {
    pub samples: u32,
    pub gpu_sum: f64,
    pub memory_sum: f64,
    pub gpu: f64,
    pub memory: f64,
}

impl AdaptiveBaseline {
    pub fn update(&mut self, gpu: f64, memory: f64) {
        if self.samples < 20 {
            self.samples += 1;
            self.gpu_sum += gpu;
            self.memory_sum += memory;
            self.gpu = self.gpu_sum / f64::from(self.samples);
            self.memory = self.memory_sum / f64::from(self.samples);
        } else {
            // Follow a falling idle floor slowly, but never chase load upward.
            if gpu < self.gpu {
                self.gpu = self.gpu * 0.98 + gpu * 0.02;
            }
            if memory < self.memory {
                self.memory = self.memory * 0.98 + memory * 0.02;
            }
        }
    }

    pub fn activity(&self, gpu: f64, memory: f64) -> f64 {
        let gpu_delta = normalize_above(gpu, self.gpu);
        let memory_delta = normalize_above(memory, self.memory);
        (gpu.max(gpu_delta) * 0.72 + memory.max(memory_delta) * 0.28).clamp(0.0, 1.0)
    }
}

pub struct App {
    pub mode: ViewMode,
    pub selected: usize,
    pub frame: u64,
    pub last_sample_time: Option<chrono::DateTime<Utc>>,
    pub baselines: BTreeMap<String, AdaptiveBaseline>,
}

impl App {
    #[must_use]
    pub fn new(mode: ViewMode) -> Self {
        Self {
            mode,
            selected: 0,
            frame: 0,
            last_sample_time: None,
            baselines: BTreeMap::new(),
        }
    }

    pub fn observe(&mut self, snapshot: &Snapshot) {
        if self.last_sample_time == Some(snapshot.captured_at) {
            return;
        }
        self.last_sample_time = Some(snapshot.captured_at);
        for device in &snapshot.devices {
            self.baselines
                .entry(device.device.id.clone())
                .or_default()
                .update(
                    device.gpu_ratio().unwrap_or(0.0),
                    device.memory_activity_ratio().unwrap_or(0.0),
                );
        }
        self.selected = self.selected.min(snapshot.devices.len().saturating_sub(1));
    }

    #[must_use]
    pub fn visual_activity(&self, device: &DeviceSnapshot) -> f64 {
        let gpu = device.gpu_ratio().unwrap_or(0.0);
        let memory = device.memory_activity_ratio().unwrap_or(0.0);
        self.baselines
            .get(&device.device.id)
            .map_or(gpu * 0.72 + memory * 0.28, |baseline| {
                baseline.activity(gpu, memory)
            })
    }

    pub fn next_mode(&mut self) {
        let index = ViewMode::ALL
            .iter()
            .position(|mode| *mode == self.mode)
            .unwrap_or(0);
        self.mode = ViewMode::ALL[(index + 1) % ViewMode::ALL.len()];
    }
}

pub fn run(collector: &Collector, initial_mode: ViewMode, fps: u16) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let result = run_loop(&mut terminal, collector, initial_mode, fps);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    collector: &Collector,
    initial_mode: ViewMode,
    fps: u16,
) -> anyhow::Result<()> {
    let mut app = App::new(initial_mode);
    let frame_interval = Duration::from_secs_f64(1.0 / f64::from(fps));

    loop {
        let started = Instant::now();
        let snapshot = collector.snapshot();
        if let Some(snapshot) = snapshot.as_ref() {
            app.observe(snapshot);
        }
        app.frame = app.frame.wrapping_add(1);
        let collection_error = collector.last_error();
        terminal
            .draw(|frame| render(frame, &app, snapshot.as_ref(), collection_error.as_deref()))?;

        let timeout = frame_interval.saturating_sub(started.elapsed());
        if event::poll(timeout)? {
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('1') => app.mode = ViewMode::Overview,
                KeyCode::Char('2') => app.mode = ViewMode::Constellation,
                KeyCode::Char('3') => app.mode = ViewMode::Memory,
                KeyCode::Char('4') => app.mode = ViewMode::Fabric,
                KeyCode::Char('5') => app.mode = ViewMode::Fleet,
                KeyCode::Tab => app.next_mode(),
                KeyCode::Right | KeyCode::Down => {
                    if let Some(snapshot) = snapshot.as_ref() {
                        app.selected =
                            (app.selected + 1).min(snapshot.devices.len().saturating_sub(1));
                    }
                }
                KeyCode::Left | KeyCode::Up => app.selected = app.selected.saturating_sub(1),
                _ => {}
            }
        }
    }
    Ok(())
}

pub fn render(
    frame: &mut ratatui::Frame<'_>,
    app: &App,
    snapshot: Option<&Snapshot>,
    collection_error: Option<&str>,
) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(1),
        ])
        .split(frame.area());

    render_header(frame, root[0], snapshot, collection_error);
    render_tabs(frame, root[1], app.mode);
    match snapshot {
        None => frame.render_widget(
            Paragraph::new("Waiting for the first NVML sample…")
                .alignment(Alignment::Center)
                .block(panel(" Telemetry ", CYAN)),
            root[2],
        ),
        Some(snapshot) if snapshot.devices.is_empty() => frame.render_widget(
            Paragraph::new("NVML returned no visible devices")
                .alignment(Alignment::Center)
                .block(panel(" Telemetry ", RED)),
            root[2],
        ),
        Some(snapshot) => {
            let selected_device = &snapshot.devices[app.selected.min(snapshot.devices.len().saturating_sub(1))];
            match app.mode {
                ViewMode::Overview => overview::render_overview(frame, root[2], app, snapshot),
                ViewMode::Constellation => constellation::render_constellation(frame, root[2], app, selected_device),
                ViewMode::Memory => memory::render_memory(frame, root[2], app, selected_device),
                ViewMode::Fabric => fabric::render_fabric(frame, root[2], snapshot),
                ViewMode::Fleet => fleet::render_fleet(frame, root[2], app, snapshot),
            }
        }
    }
    frame.render_widget(
        Paragraph::new(" 1 Overview  2 SMs  3 Memory  4 Fabric  5 Fleet  ←/→ GPU  Tab Cycle  q Quit")
            .style(Style::default().fg(MUTED)),
        root[3],
    );
}

fn render_header(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: Option<&Snapshot>,
    collection_error: Option<&str>,
) {
    let (versions, status, status_color) = snapshot.map_or_else(
        || ("NVML initializing".to_owned(), "WAIT".to_owned(), AMBER),
        |snapshot| {
            let versions = format!(
                "driver {} · NVML {} · {} device{}",
                snapshot.driver_version.as_deref().unwrap_or("?"),
                snapshot.nvml_version.as_deref().unwrap_or("?"),
                snapshot.devices.len(),
                if snapshot.devices.len() == 1 { "" } else { "s" }
            );
            let age_ms = Utc::now()
                .signed_duration_since(snapshot.captured_at)
                .num_milliseconds()
                .max(0);
            if collection_error.is_some() {
                (versions, format!("STALE · {age_ms}ms"), RED)
            } else {
                (versions, format!("LIVE · {age_ms}ms"), GREEN)
            }
        },
    );
    let line = Line::from(vec![
        Span::styled(
            " nv-toplike ",
            Style::default()
                .fg(Color::Black)
                .bg(CYAN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(versions, Style::default().fg(Color::White)),
        Span::raw("  "),
        Span::styled(
            status,
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(collection_error.map_or("", |_| " · last good sample retained")),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_tabs(frame: &mut ratatui::Frame<'_>, area: Rect, selected: ViewMode) {
    let selected_index = ViewMode::ALL
        .iter()
        .position(|mode| *mode == selected)
        .unwrap_or(0);
    let tabs = Tabs::new(
        ViewMode::ALL
            .iter()
            .enumerate()
            .map(|(index, mode)| Line::from(format!(" {} {} ", index + 1, mode.title())))
            .collect::<Vec<_>>(),
    )
    .select(selected_index)
    .highlight_style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD))
    .divider(Span::styled("│", Style::default().fg(MUTED)))
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(MUTED)),
    );
    frame.render_widget(tabs, area);
}

pub fn render_gauge(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    ratio: Option<f64>,
    color: Color,
) {
    let label = ratio.map_or_else(
        || "N/A".to_owned(),
        |value| format!("{:.0}%", value * 100.0),
    );
    let gauge = Gauge::default()
        .block(panel(title, color))
        .gauge_style(Style::default().fg(color).bg(Color::Rgb(25, 28, 38)))
        .ratio(ratio.unwrap_or(0.0).clamp(0.0, 1.0))
        .label(label);
    frame.render_widget(gauge, area);
}

pub fn panel(title: &str, color: Color) -> Block<'_> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
}

pub fn normalize_above(value: f64, baseline: f64) -> f64 {
    if baseline >= 1.0 {
        0.0
    } else {
        ((value - baseline) / (1.0 - baseline)).clamp(0.0, 1.0)
    }
}

pub fn animated_link(label: &str, rate: Option<u64>, frame: u64, rightward: bool) -> String {
    let track_width = 13;
    let density = rate.map_or(0, |value| match value {
        0 => 0,
        1..=1_048_576 => 1,
        1_048_577..=104_857_600 => 2,
        _ => 3,
    });
    let mut track = vec!['─'; track_width];
    for particle in 0..density {
        let position = ((frame as usize * (particle + 1) + particle * 7) % track_width).min(track_width - 1);
        if rightward {
            track[position] = '◆';
        } else {
            track[track_width - 1 - position] = '◆';
        }
    }
    let track_str: String = track.into_iter().collect();
    let track_with_arrow = if rightward {
        format!("{track_str}▶")
    } else {
        format!("◀{track_str}")
    };
    let rate_str = format!("{:>10}", fmt_rate(rate));
    format!("{label:<12}  {track_with_arrow}  {rate_str}")
}

pub fn short_uuid(uuid: &str) -> String {
    if uuid.width() <= 18 {
        return uuid.to_owned();
    }
    let prefix = uuid.chars().take(10).collect::<String>();
    let suffix_rev = uuid.chars().rev().take(5).collect::<String>();
    let suffix = suffix_rev.chars().rev().collect::<String>();
    format!("{prefix}…{suffix}")
}

pub fn short_name(name: &str, width: usize) -> String {
    if name.width() <= width {
        return name.to_owned();
    }
    if width <= 1 {
        return "…".to_owned();
    }
    let mut out = String::new();
    for ch in name.chars() {
        if out.as_str().width() + ch.to_string().as_str().width() >= width {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

pub fn fmt_percent(value: Option<f64>) -> String {
    value.map_or_else(
        || "N/A".to_owned(),
        |value| format!("{:.0}%", value * 100.0),
    )
}

pub fn fmt_temp(value: Option<f64>) -> String {
    value.map_or_else(|| "N/A".to_owned(), |value| format!("{value:.0}°C"))
}

pub fn fmt_watts(value: Option<f64>) -> String {
    value.map_or_else(|| "N/A".to_owned(), |value| format!("{value:.1}W"))
}

pub fn fmt_mhz(value: Option<u32>) -> String {
    value.map_or_else(|| "N/A".to_owned(), |value| format!("{value}MHz"))
}

pub fn fmt_rate(value: Option<u64>) -> String {
    value.map_or_else(
        || "N/A".to_owned(),
        |value| format!("{}/s", human_bytes(value)),
    )
}

pub fn fmt_bytes(value: Option<u64>) -> String {
    value.map_or_else(|| "N/A".to_owned(), human_bytes)
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes}{}", UNITS[unit])
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    use ratatui::backend::TestBackend;

    use crate::model::{
        AcceleratorDevice, AcceleratorSample, DeviceSnapshot, EntityKind, MemorySample, Metric,
        MetricScope, Snapshot,
    };

    #[test]
    fn byte_formatting_uses_iec_units() {
        assert_eq!(human_bytes(0), "0B");
        assert_eq!(human_bytes(1_024), "1.0KiB");
        assert_eq!(human_bytes(10 * 1_024 * 1_024), "10.0MiB");
    }

    #[test]
    fn short_name_respects_unicode_display_width() {
        let shortened = short_name("GPU ⛩ Blackwell", 9);
        assert!(shortened.as_str().width() <= 9);
        assert!(shortened.ends_with('…'));
    }

    #[test]
    fn baseline_does_not_turn_load_into_idle() {
        let mut baseline = AdaptiveBaseline::default();
        for _ in 0..20 {
            baseline.update(0.05, 0.02);
        }
        assert!(baseline.activity(0.8, 0.5) > 0.7);
    }

    #[test]
    fn animated_link_is_directional_and_fixed_width() {
        let right = animated_link("TX", Some(10_000_000), 4, true);
        let left = animated_link("RX", Some(445_440), 4, false);
        let right_zero = animated_link("TX", Some(0), 4, true);
        let left_none = animated_link("RX", None, 4, false);
        assert!(right.contains('▶'));
        assert!(left.contains('◀'));
        assert_eq!(right.chars().count(), left.chars().count());
        assert_eq!(right.chars().count(), right_zero.chars().count());
        assert_eq!(right.chars().count(), left_none.chars().count());
    }

    #[test]
    fn every_view_renders_at_practical_terminal_sizes() {
        for (width, height) in [(80, 24), (120, 40), (160, 50)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).expect("test terminal");
            let snapshot = fixture_snapshot();
            let mut app = App::new(ViewMode::Overview);
            app.observe(&snapshot);
            for mode in ViewMode::ALL {
                app.mode = mode;
                terminal
                    .draw(|frame| render(frame, &app, Some(&snapshot), None))
                    .expect("view renders");
            }
        }
    }

    #[test]
    fn constellation_renders_multi_row_with_large_sm_count() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut snapshot = fixture_snapshot();
        snapshot.devices[0].device.compute_units = Some(188); // Blackwell RTX PRO 6000
        let mut app = App::new(ViewMode::Constellation);
        app.observe(&snapshot);
        terminal
            .draw(|frame| render(frame, &app, Some(&snapshot), None))
            .expect("constellation renders");
    }

    #[test]
    fn fabric_renders_single_and_multi_gpu() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let snapshot = fixture_snapshot();
        let mut app = App::new(ViewMode::Fabric);
        app.observe(&snapshot);
        terminal
            .draw(|frame| render(frame, &app, Some(&snapshot), None))
            .expect("fabric single gpu renders");
    }

    #[test]
    fn print_rendered_views_for_inspection() {
        let backend = TestBackend::new(120, 32);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut snapshot = fixture_snapshot();
        snapshot.devices[0].device.compute_units = Some(188);
        let mut app = App::new(ViewMode::Constellation);
        app.observe(&snapshot);

        for mode in [ViewMode::Overview, ViewMode::Constellation, ViewMode::Memory, ViewMode::Fabric, ViewMode::Fleet] {
            app.mode = mode;
            terminal
                .draw(|frame| render(frame, &app, Some(&snapshot), None))
                .expect("render");
            println!("\n=== VIEW: {:?} ===", mode);
            let buffer = terminal.backend().buffer();
            for y in 0..buffer.area.height {
                let mut line = String::new();
                for x in 0..buffer.area.width {
                    line.push_str(buffer[(x, y)].symbol());
                }
                println!("{}", line);
            }
        }
    }

    fn fixture_snapshot() -> Snapshot {
        let now = Utc::now();
        let mut sample = AcceleratorSample::default();
        sample.utilization.gpu_ratio = Some(Metric::nvml(0.42, now, MetricScope::Device));
        sample.utilization.memory_controller_ratio =
            Some(Metric::nvml(0.25, now, MetricScope::Device));
        sample.memory = MemorySample {
            used_bytes: Some(Metric::nvml(4 * 1_024_u64.pow(3), now, MetricScope::Device)),
            total_bytes: Some(Metric::nvml(
                16 * 1_024_u64.pow(3),
                now,
                MetricScope::Device,
            )),
            free_bytes: Some(Metric::nvml(
                12 * 1_024_u64.pow(3),
                now,
                MetricScope::Device,
            )),
            reserved_bytes: Some(Metric::nvml(0, now, MetricScope::Device)),
        };
        Snapshot {
            devices: vec![DeviceSnapshot {
                device: AcceleratorDevice {
                    id: "GPU-fixture".to_owned(),
                    parent_id: None,
                    display_index: Some(0),
                    pci_bus_id: Some("0000:01:00.0".to_owned()),
                    vendor: "NVIDIA".to_owned(),
                    name: "Fixture Blackwell GPU".to_owned(),
                    architecture: Some("Blackwell".to_owned()),
                    entity_kind: EntityKind::PhysicalGpu,
                    compute_units: Some(16),
                    memory_total_bytes: Some(16 * 1_024_u64.pow(3)),
                    mig_enabled: Some(false),
                    capabilities: BTreeSet::new(),
                },
                sample,
                processes: Vec::new(),
                stale: false,
                error: None,
            }],
            ..Snapshot::empty(now)
        }
    }
}
