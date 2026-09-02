// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::io;
use std::time::{Duration, Instant};

use chrono::Utc;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Gauge, Paragraph, Row, Table, Tabs, Wrap,
};
use unicode_width::UnicodeWidthStr;

use crate::cli::ViewMode;
use crate::collector::Collector;
use crate::model::{DeviceSnapshot, ProcessKind, Snapshot, TopologyKind};

const CYAN: Color = Color::Rgb(90, 220, 255);
const GREEN: Color = Color::Rgb(120, 255, 170);
const AMBER: Color = Color::Rgb(255, 200, 90);
const PINK: Color = Color::Rgb(255, 120, 205);
const MUTED: Color = Color::Rgb(120, 130, 155);
const RED: Color = Color::Rgb(255, 90, 90);

#[derive(Debug, Clone, Default)]
struct AdaptiveBaseline {
    samples: u32,
    gpu_sum: f64,
    memory_sum: f64,
    gpu: f64,
    memory: f64,
}

impl AdaptiveBaseline {
    fn update(&mut self, gpu: f64, memory: f64) {
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

    fn activity(&self, gpu: f64, memory: f64) -> f64 {
        let gpu_delta = normalize_above(gpu, self.gpu);
        let memory_delta = normalize_above(memory, self.memory);
        (gpu.max(gpu_delta) * 0.72 + memory.max(memory_delta) * 0.28).clamp(0.0, 1.0)
    }
}

struct App {
    mode: ViewMode,
    selected: usize,
    frame: u64,
    last_sample_time: Option<chrono::DateTime<Utc>>,
    baselines: BTreeMap<String, AdaptiveBaseline>,
}

impl App {
    fn new(mode: ViewMode) -> Self {
        Self {
            mode,
            selected: 0,
            frame: 0,
            last_sample_time: None,
            baselines: BTreeMap::new(),
        }
    }

    fn observe(&mut self, snapshot: &Snapshot) {
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

    fn visual_activity(&self, device: &DeviceSnapshot) -> f64 {
        let gpu = device.gpu_ratio().unwrap_or(0.0);
        let memory = device.memory_activity_ratio().unwrap_or(0.0);
        self.baselines
            .get(&device.device.id)
            .map_or(gpu * 0.72 + memory * 0.28, |baseline| {
                baseline.activity(gpu, memory)
            })
    }

    fn next_mode(&mut self) {
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

fn render(
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
        Some(snapshot) => match app.mode {
            ViewMode::Overview => render_overview(frame, root[2], app, snapshot),
            ViewMode::Constellation => render_constellation(frame, root[2], app, snapshot),
            ViewMode::Memory => render_memory(frame, root[2], app, snapshot),
            ViewMode::Fabric => render_fabric(frame, root[2], snapshot),
            ViewMode::Fleet => render_fleet(frame, root[2], app, snapshot),
        },
    }
    frame.render_widget(
        Paragraph::new(" 1 Overview  2 SMs  3 Memory  4 Fabric  5 Fleet  ←→ GPU  Tab view  q quit")
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

fn render_overview(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App, snapshot: &Snapshot) {
    let device = &snapshot.devices[app.selected];
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(5),
            Constraint::Min(5),
        ])
        .split(area);
    render_device_selector(frame, layout[0], snapshot, app.selected);

    let gauges = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25); 4])
        .split(layout[1]);
    render_gauge(
        frame,
        gauges[0],
        " GPU kernel activity ",
        device.gpu_ratio(),
        CYAN,
    );
    render_gauge(
        frame,
        gauges[1],
        " Memory controller ",
        device.memory_activity_ratio(),
        PINK,
    );
    render_gauge(
        frame,
        gauges[2],
        " VRAM allocation ",
        device.memory_fill_ratio(),
        AMBER,
    );
    render_gauge(
        frame,
        gauges[3],
        " Power limit ",
        device.power_ratio(),
        GREEN,
    );

    let lower = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(43), Constraint::Percentage(57)])
        .split(layout[2]);
    render_details(frame, lower[0], device);
    render_processes(frame, lower[1], device);
}

fn render_device_selector(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &Snapshot,
    selected: usize,
) {
    let spans = snapshot
        .devices
        .iter()
        .enumerate()
        .flat_map(|(index, device)| {
            let style = if index == selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(CYAN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let label = format!(
                " {}:{} {:.0}% {:.0}°C ",
                device.device.display_index.unwrap_or(index as u32),
                short_name(&device.device.name, 18),
                device.gpu_ratio().unwrap_or(0.0) * 100.0,
                device.temperature_c().unwrap_or(0.0)
            );
            [Span::styled(label, style), Span::raw(" ")]
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .wrap(Wrap { trim: true })
            .block(panel(" GPUs · stable identity is UUID ", CYAN)),
        area,
    );
}

fn render_details(frame: &mut ratatui::Frame<'_>, area: Rect, device: &DeviceSnapshot) {
    let sample = &device.sample;
    let power = sample.power.power_watts.as_ref().map(|m| m.value);
    let limit = sample.power.power_limit_watts.as_ref().map(|m| m.value);
    let temperature = device.temperature_c();
    let fan = sample.thermals.fan_percent.as_ref().map(|m| m.value);
    let sm_clock = sample.clocks.sm_clock_mhz.as_ref().map(|m| m.value);
    let memory_clock = sample.clocks.memory_clock_mhz.as_ref().map(|m| m.value);
    let pcie_tx = sample
        .links
        .pcie_tx_bytes_per_second
        .as_ref()
        .map(|m| m.value);
    let pcie_rx = sample
        .links
        .pcie_rx_bytes_per_second
        .as_ref()
        .map(|m| m.value);
    let health = sample.health.observations.join(" · ");
    let lines =
        vec![
            Line::from(vec![
                Span::styled("Model  ", Style::default().fg(MUTED)),
                Span::raw(&device.device.name),
            ]),
            Line::from(format!(
                "UUID   {}  ·  PCI {}",
                short_uuid(&device.device.id),
                device.device.pci_bus_id.as_deref().unwrap_or("N/A")
            )),
            Line::from(format!(
                "Power  {} / {}  ·  Temp {}  ·  Fan {}",
                fmt_watts(power),
                fmt_watts(limit),
                fmt_temp(temperature),
                fmt_percent(fan.map(|value| value / 100.0))
            )),
            Line::from(format!(
                "Clock  SM {}  ·  MEM {}  ·  {}",
                fmt_mhz(sm_clock),
                fmt_mhz(memory_clock),
                sample.clocks.performance_state.as_deref().unwrap_or("P?")
            )),
            Line::from(format!(
                "PCIe   TX {}  ·  RX {}  ·  Gen{} x{}",
                fmt_rate(pcie_tx),
                fmt_rate(pcie_rx),
                sample
                    .links
                    .pcie_generation
                    .map_or_else(|| "?".to_owned(), |v| v.to_string()),
                sample
                    .links
                    .pcie_width
                    .map_or_else(|| "?".to_owned(), |v| v.to_string())
            )),
            Line::from(vec![
                Span::styled("Health ", Style::default().fg(MUTED)),
                Span::styled(
                    health,
                    Style::default().fg(
                        if sample.health.observations.iter().any(|value| {
                            value.contains("uncorrected") || value.contains("slowdown")
                        }) {
                            RED
                        } else {
                            GREEN
                        },
                    ),
                ),
            ]),
        ];
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(panel(" Device ", CYAN)),
        area,
    );
}

fn render_processes(frame: &mut ratatui::Frame<'_>, area: Rect, device: &DeviceSnapshot) {
    let rows = device.processes.iter().map(|process| {
        Row::new(vec![
            Cell::from(process.pid.to_string()),
            Cell::from(match process.kind {
                ProcessKind::Compute => "C",
                ProcessKind::Graphics => "G",
                ProcessKind::ComputeAndGraphics => "C+G",
            }),
            Cell::from(process.command.as_deref().unwrap_or("unknown")),
            Cell::from(fmt_bytes(process.used_gpu_memory_bytes)),
            Cell::from(fmt_percent(process.sm_ratio)),
            Cell::from(fmt_percent(process.memory_ratio)),
        ])
    });
    let title = format!(" Processes · {} ", device.processes.len());
    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Length(5),
            Constraint::Min(12),
            Constraint::Length(10),
            Constraint::Length(7),
            Constraint::Length(7),
        ],
    )
    .header(
        Row::new(["PID", "TYPE", "COMMAND", "VRAM", "SM", "MEM"])
            .style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
    )
    .block(panel(&title, PINK))
    .column_spacing(1);
    frame.render_widget(table, area);
}

fn render_constellation(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    app: &App,
    snapshot: &Snapshot,
) {
    let device = &snapshot.devices[app.selected];
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(area);
    let reported_sm_count = device.device.compute_units.map(|count| count as usize);
    // NVML's physical-device attributes call is not supported by every driver
    // and SKU (including some current Blackwell workstation cards). Keep the
    // view useful with an explicitly illustrative field instead of deriving an
    // SM count from marketing CUDA-core totals.
    let display_units = reported_sm_count.unwrap_or(64).max(1);
    let activity = app.visual_activity(device);
    let tensor = device
        .sample
        .utilization
        .tensor_active_ratio
        .as_ref()
        .map(|m| m.value);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} ", short_name(&device.device.name, 34)),
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "{} · aggregate GPU activity {} · Tensor {}",
                reported_sm_count.map_or_else(
                    || "SM count unavailable".to_owned(),
                    |count| format!("{count} SMs")
                ),
                fmt_percent(device.gpu_ratio()),
                fmt_percent(tensor)
            )),
        ]))
        .block(panel(" SM Constellation ", CYAN)),
        layout[0],
    );

    let inner_width = layout[1].width.saturating_sub(2) as usize;
    let inner_height = layout[1].height.saturating_sub(2) as usize;
    let columns = (inner_width / 2).max(1);
    let visible = display_units.min(columns.saturating_mul(inner_height));
    let temperature = device.temperature_c().unwrap_or(35.0);
    let color = heat_color(temperature);
    let glyphs = ["·", "∘", "○", "◉", "●"];
    let mut lines = Vec::new();
    for row in 0..visible.div_ceil(columns) {
        let mut spans = Vec::new();
        for column in 0..columns {
            let index = row * columns + column;
            if index >= visible {
                break;
            }
            // Positional phase is decorative; every cell is still driven by
            // the same aggregate activity value.
            let phase = ((app.frame + (index as u64 * 7)) % 17) as f64 / 17.0;
            let level = ((activity * 4.2 + phase * 0.45).floor() as usize).min(4);
            spans.push(Span::styled(
                format!("{} ", glyphs[level]),
                Style::default().fg(color),
            ));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(panel(" Aggregate SM field ", heat_color(temperature))),
        layout[1],
    );
    frame.render_widget(
        Paragraph::new(format!(
            "Illustrative layout: all cells share device-level NVML activity. {}",
            reported_sm_count.map_or_else(
                || format!("{visible} display cells; NVML did not expose the physical SM count."),
                |count| format!("Showing {visible}/{count} reported SMs.")
            )
        ))
        .style(Style::default().fg(MUTED))
        .alignment(Alignment::Center),
        layout[2],
    );
}

fn render_memory(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App, snapshot: &Snapshot) {
    let device = &snapshot.devices[app.selected];
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(format!(
            "{} · VRAM {} / {} · memory-controller activity {}",
            device.device.name,
            fmt_bytes(
                device
                    .sample
                    .memory
                    .used_bytes
                    .as_ref()
                    .map(|metric| metric.value)
            ),
            fmt_bytes(
                device
                    .sample
                    .memory
                    .total_bytes
                    .as_ref()
                    .map(|metric| metric.value)
            ),
            fmt_percent(device.memory_activity_ratio())
        ))
        .block(panel(" Memory Foundry ", PINK)),
        layout[0],
    );

    let transport = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(24),
            Constraint::Percentage(52),
            Constraint::Percentage(24),
        ])
        .split(layout[1]);
    frame.render_widget(
        Paragraph::new("HOST\nMEMORY")
            .alignment(Alignment::Center)
            .block(panel(" source/sink ", MUTED)),
        transport[0],
    );
    let tx = device
        .sample
        .links
        .pcie_tx_bytes_per_second
        .as_ref()
        .map(|metric| metric.value);
    let rx = device
        .sample
        .links
        .pcie_rx_bytes_per_second
        .as_ref()
        .map(|metric| metric.value);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(animated_link("GPU TX", tx, app.frame, true)),
            Line::from(animated_link(
                "GPU RX",
                rx,
                app.frame.wrapping_add(5),
                false,
            )),
        ])
        .alignment(Alignment::Center)
        .block(panel(" measured PCIe throughput ", CYAN)),
        transport[1],
    );
    frame.render_widget(
        Paragraph::new("GPU\nVRAM")
            .alignment(Alignment::Center)
            .block(panel(" destination/source ", AMBER)),
        transport[2],
    );

    render_gauge(
        frame,
        layout[2],
        " VRAM allocation · persistent reservoir, not traffic ",
        device.memory_fill_ratio(),
        AMBER,
    );
    let activity = device.memory_activity_ratio();
    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .split(layout[3]);
    render_foundry_stage(frame, inner[0], "VRAM / HBM", activity, app.frame, PINK);
    render_foundry_stage(
        frame,
        inner[1],
        "L2 context",
        activity,
        app.frame.wrapping_add(4),
        AMBER,
    );
    render_foundry_stage(
        frame,
        inner[2],
        "SM local context",
        Some(app.visual_activity(device)),
        app.frame.wrapping_add(8),
        CYAN,
    );
    frame.render_widget(
        Paragraph::new(
            "Internal L2/SM motion is architectural context; NVML measures aggregate global-memory activity, not cache hops.",
        )
        .style(Style::default().fg(MUTED))
        .alignment(Alignment::Center),
        layout[4],
    );
}

fn render_foundry_stage(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    label: &str,
    ratio: Option<f64>,
    phase: u64,
    color: Color,
) {
    let ratio = ratio.unwrap_or(0.0).clamp(0.0, 1.0);
    let width = area.width.saturating_sub(4) as usize;
    let lit = ((width as f64) * ratio).round() as usize;
    let mut line = String::with_capacity(width);
    for index in 0..width {
        let ch = if index < lit {
            if (index as u64 + phase).is_multiple_of(5) {
                '◆'
            } else {
                '▓'
            }
        } else {
            '░'
        };
        line.push(ch);
    }
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(line),
            Line::from(format!("{} activity", fmt_percent(Some(ratio)))),
        ])
        .alignment(Alignment::Center)
        .style(Style::default().fg(color))
        .block(panel(&format!(" {label} "), color)),
        area,
    );
}

fn render_fabric(frame: &mut ratatui::Frame<'_>, area: Rect, snapshot: &Snapshot) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(4)])
        .split(area);
    let name_by_id = snapshot
        .devices
        .iter()
        .map(|device| {
            (
                device.device.id.as_str(),
                format!(
                    "GPU{} {}",
                    device.device.display_index.unwrap_or(0),
                    short_name(&device.device.name, 28)
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut lines = snapshot
        .devices
        .iter()
        .map(|device| {
            Line::from(vec![
                Span::styled(
                    "● ",
                    Style::default().fg(heat_color(device.temperature_c().unwrap_or(35.0))),
                ),
                Span::styled(
                    name_by_id
                        .get(device.device.id.as_str())
                        .cloned()
                        .unwrap_or_else(|| short_uuid(&device.device.id)),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(
                    "  {}  PCIe Gen{} x{}  TX {}  RX {}",
                    device.device.pci_bus_id.as_deref().unwrap_or("PCI N/A"),
                    device
                        .sample
                        .links
                        .pcie_generation
                        .map_or_else(|| "?".to_owned(), |v| v.to_string()),
                    device
                        .sample
                        .links
                        .pcie_width
                        .map_or_else(|| "?".to_owned(), |v| v.to_string()),
                    fmt_rate(
                        device
                            .sample
                            .links
                            .pcie_tx_bytes_per_second
                            .as_ref()
                            .map(|m| m.value)
                    ),
                    fmt_rate(
                        device
                            .sample
                            .links
                            .pcie_rx_bytes_per_second
                            .as_ref()
                            .map(|m| m.value)
                    ),
                )),
            ])
        })
        .collect::<Vec<_>>();
    if snapshot.topology.is_empty() {
        lines.push(Line::from(Span::styled(
            "\nSingle visible GPU; no inter-GPU topology edge to draw.",
            Style::default().fg(MUTED),
        )));
    } else {
        lines.push(Line::from(""));
        for edge in &snapshot.topology {
            lines.push(Line::from(vec![
                Span::styled("  └─ ", Style::default().fg(MUTED)),
                Span::raw(
                    name_by_id
                        .get(edge.from.as_str())
                        .cloned()
                        .unwrap_or_else(|| short_uuid(&edge.from)),
                ),
                Span::styled(
                    format!(" ──{:?}── ", edge.kind),
                    Style::default().fg(topology_color(&edge.kind)),
                ),
                Span::raw(
                    name_by_id
                        .get(edge.to.as_str())
                        .cloned()
                        .unwrap_or_else(|| short_uuid(&edge.to)),
                ),
            ]));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(panel(
                " Fabric Map · relationships are direct NVML topology ",
                CYAN,
            )),
        layout[0],
    );
    frame.render_widget(
        Paragraph::new(
            "PCIe rates are per-device NVML measurements. A static topology edge does not imply measured peer traffic; NVLink enrichment is pending DCGM support.",
        )
        .style(Style::default().fg(MUTED))
        .wrap(Wrap { trim: true })
        .block(panel(" Semantics ", MUTED)),
        layout[1],
    );
}

fn render_fleet(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App, snapshot: &Snapshot) {
    let rows = snapshot.devices.iter().enumerate().map(|(index, device)| {
        let health_fault = device.sample.health.observations.iter().any(|value| {
            value.contains("uncorrected") || value.contains("slowdown") || value.contains("brake")
        });
        Row::new(vec![
            Cell::from(if index == app.selected { "▶" } else { " " }),
            Cell::from(
                device
                    .device
                    .display_index
                    .map_or_else(|| "-".to_owned(), |v| v.to_string()),
            ),
            Cell::from(short_name(&device.device.name, 32)),
            Cell::from(device.device.architecture.as_deref().unwrap_or("N/A")),
            Cell::from(fmt_percent(device.gpu_ratio())),
            Cell::from(fmt_percent(device.memory_activity_ratio())),
            Cell::from(format!(
                "{} / {}",
                fmt_bytes(device.sample.memory.used_bytes.as_ref().map(|m| m.value)),
                fmt_bytes(device.sample.memory.total_bytes.as_ref().map(|m| m.value))
            )),
            Cell::from(fmt_watts(
                device.sample.power.power_watts.as_ref().map(|m| m.value),
            )),
            Cell::from(fmt_temp(device.temperature_c())),
            Cell::from(if health_fault { "WARN" } else { "OBS OK" }),
        ])
        .style(if health_fault {
            Style::default().fg(RED)
        } else if index == app.selected {
            Style::default().fg(CYAN)
        } else {
            Style::default()
        })
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(4),
            Constraint::Min(18),
            Constraint::Length(11),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(21),
            Constraint::Length(9),
            Constraint::Length(7),
            Constraint::Length(8),
        ],
    )
    .header(
        Row::new([
            "", "GPU", "MODEL", "ARCH", "GPU", "MEM", "VRAM", "POWER", "TEMP", "HEALTH",
        ])
        .style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
    )
    .column_spacing(1)
    .block(panel(
        " Fleet · UUID-stable ordering from NVML enumeration ",
        CYAN,
    ));
    frame.render_widget(table, area);
}

fn render_gauge(
    frame: &mut ratatui::Frame<'_>,
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

fn panel(title: &str, color: Color) -> Block<'_> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
}

fn normalize_above(value: f64, baseline: f64) -> f64 {
    if baseline >= 1.0 {
        0.0
    } else {
        ((value - baseline) / (1.0 - baseline)).clamp(0.0, 1.0)
    }
}

fn heat_color(temperature: f64) -> Color {
    if temperature >= 85.0 {
        RED
    } else if temperature >= 70.0 {
        Color::Rgb(255, 120, 80)
    } else if temperature >= 55.0 {
        AMBER
    } else if temperature >= 40.0 {
        GREEN
    } else {
        CYAN
    }
}

fn topology_color(kind: &TopologyKind) -> Color {
    match kind {
        TopologyKind::MigParent | TopologyKind::PciInternal => GREEN,
        TopologyKind::PciSingleSwitch => CYAN,
        TopologyKind::PciMultiSwitch | TopologyKind::PciHostBridge => AMBER,
        TopologyKind::NumaNode | TopologyKind::System | TopologyKind::Unknown => MUTED,
    }
}

fn animated_link(label: &str, rate: Option<u64>, frame: u64, rightward: bool) -> String {
    let width = 20;
    let density = rate.map_or(0, |value| match value {
        0 => 0,
        1..=1_048_576 => 1,
        1_048_577..=104_857_600 => 2,
        _ => 3,
    });
    let mut track = vec!['─'; width];
    for particle in 0..density {
        let position = ((frame as usize * (particle + 1) + particle * 7) % width).min(width - 1);
        track[if rightward {
            position
        } else {
            width - 1 - position
        }] = '◆';
    }
    let arrow = if rightward { '▶' } else { '◀' };
    format!(
        "{label:6} {}{arrow}  {}",
        track.into_iter().collect::<String>(),
        fmt_rate(rate)
    )
}

fn short_uuid(uuid: &str) -> String {
    if uuid.width() <= 18 {
        return uuid.to_owned();
    }
    let prefix = uuid.chars().take(10).collect::<String>();
    let suffix_rev = uuid.chars().rev().take(5).collect::<String>();
    let suffix = suffix_rev.chars().rev().collect::<String>();
    format!("{prefix}…{suffix}")
}

fn short_name(name: &str, width: usize) -> String {
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

fn fmt_percent(value: Option<f64>) -> String {
    value.map_or_else(
        || "N/A".to_owned(),
        |value| format!("{:.0}%", value * 100.0),
    )
}

fn fmt_temp(value: Option<f64>) -> String {
    value.map_or_else(|| "N/A".to_owned(), |value| format!("{value:.0}°C"))
}

fn fmt_watts(value: Option<f64>) -> String {
    value.map_or_else(|| "N/A".to_owned(), |value| format!("{value:.1}W"))
}

fn fmt_mhz(value: Option<u32>) -> String {
    value.map_or_else(|| "N/A".to_owned(), |value| format!("{value}MHz"))
}

fn fmt_rate(value: Option<u64>) -> String {
    value.map_or_else(
        || "N/A".to_owned(),
        |value| format!("{}/s", human_bytes(value)),
    )
}

fn fmt_bytes(value: Option<u64>) -> String {
    value.map_or_else(|| "N/A".to_owned(), human_bytes)
}

fn human_bytes(bytes: u64) -> String {
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
        let left = animated_link("RX", Some(10_000_000), 4, false);
        assert!(right.contains('▶'));
        assert!(left.contains('◀'));
    }

    #[test]
    fn every_view_renders_at_practical_terminal_sizes() {
        for (width, height) in [(80, 24), (120, 40)] {
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
