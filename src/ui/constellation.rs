// SPDX-License-Identifier: Apache-2.0

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::model::DeviceSnapshot;
use crate::ui::colors::*;
use crate::ui::{
    App, fmt_bytes, fmt_mhz, fmt_percent, fmt_rate, fmt_temp, fmt_watts, panel, short_name,
};

const GLYPHS: [char; 5] = ['·', '∘', '○', '◉', '●'];

struct SmGridParams {
    display_units: usize,
    activity: f64,
    tensor: f64,
    temperature: f64,
    temp_hue: f64,
}

pub fn render_constellation(frame: &mut Frame<'_>, area: Rect, app: &App, device: &DeviceSnapshot) {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(4),
        ])
        .split(area);

    let reported_sm_count = device.device.compute_units.map(|count| count as usize);
    let display_units = reported_sm_count.unwrap_or(64).max(1);
    let activity = app.visual_activity(device);
    let tensor = device
        .sample
        .utilization
        .tensor_active_ratio
        .as_ref()
        .map(|m| m.value)
        .unwrap_or(0.0);
    let temperature = device.temperature_c().unwrap_or(35.0);
    let temp_hue = temp_to_hue(temperature);

    // Top Header HUD
    let header_line = Line::from(vec![
        Span::styled(
            format!(" {} ", short_name(&device.device.name, 36)),
            Style::default()
                .fg(Color::Black)
                .bg(CYAN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            reported_sm_count
                .map_or_else(|| "SM Count N/A".to_owned(), |count| format!("{count} SMs")),
            Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · Arch ", Style::default().fg(MUTED)),
        Span::styled(
            device.device.architecture.as_deref().unwrap_or("NVIDIA"),
            Style::default().fg(WHITE),
        ),
        Span::styled(" · Kernel Activity ", Style::default().fg(MUTED)),
        Span::styled(
            fmt_percent(device.gpu_ratio()),
            Style::default()
                .fg(util_color(device.gpu_ratio().unwrap_or(0.0)))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · Baseline Activity ", Style::default().fg(MUTED)),
        Span::styled(
            format!("{:.0}%", activity * 100.0),
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · Tensor ", Style::default().fg(MUTED)),
        Span::styled(
            if tensor > 0.0 {
                format!("{:.0}%", tensor * 100.0)
            } else {
                "Idle".to_owned()
            },
            Style::default().fg(if tensor > 0.0 { PINK } else { MUTED }),
        ),
    ]);

    frame.render_widget(
        Paragraph::new(header_line)
            .block(panel(" SM Constellation · Aggregate Compute Fabric ", CYAN)),
        main_layout[0],
    );

    // Split middle into SM Grid (left/center) and Live Telemetry Deck (right)
    let body_chunks = if main_layout[1].width >= 90 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(66), Constraint::Percentage(34)])
            .split(main_layout[1])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100), Constraint::Length(0)])
            .split(main_layout[1])
    };

    let params = SmGridParams {
        display_units,
        activity,
        tensor,
        temperature,
        temp_hue,
    };
    render_sm_grid(frame, body_chunks[0], app, &params);

    if body_chunks[1].width > 15 {
        render_telemetry_deck(frame, body_chunks[1], device, activity, tensor, temperature);
    }

    // Bottom Horizon Panel
    render_bottom_horizon(
        frame,
        main_layout[2],
        device,
        reported_sm_count,
        display_units,
    );
}

fn render_sm_grid(frame: &mut Frame<'_>, area: Rect, app: &App, params: &SmGridParams) {
    let inner_width = area.width.saturating_sub(4) as usize;
    let inner_height = area.height.saturating_sub(3) as usize;
    let display_units = params.display_units;

    // Determine optimal clustering and columns to form a balanced multi-row 2D grid
    let cluster_size = if display_units > 48 {
        4
    } else if display_units > 16 {
        2
    } else {
        1
    };

    // Calculate cluster character width: e.g. "[ · · · · ] " = 2 + 4*2 + 2 = 12 chars
    let cluster_char_width = 2 + cluster_size * 2 + 1;
    let max_clusters_per_row = (inner_width / cluster_char_width).max(1);

    let total_clusters = display_units.div_ceil(cluster_size);
    // Find clusters per row that produces a balanced height
    let desired_rows = inner_height.clamp(2, 16);
    let target_clusters_per_row = total_clusters
        .div_ceil(desired_rows)
        .clamp(1, max_clusters_per_row);
    let clusters_per_row = target_clusters_per_row.min(max_clusters_per_row);
    let total_rows = total_clusters.div_ceil(clusters_per_row);

    let mut lines = Vec::new();

    for row_idx in 0..total_rows {
        let mut spans = Vec::new();
        let start_sm = row_idx * clusters_per_row * cluster_size;
        spans.push(Span::styled(
            format!("SM{:03}│ ", start_sm),
            Style::default().fg(MUTED),
        ));

        for cluster_idx in 0..clusters_per_row {
            let global_cluster = row_idx * clusters_per_row + cluster_idx;
            if global_cluster >= total_clusters {
                break;
            }

            spans.push(Span::styled("[ ", Style::default().fg(DARK_GRAY)));

            for sm_offset in 0..cluster_size {
                let sm_idx = global_cluster * cluster_size + sm_offset;
                if sm_idx >= display_units {
                    spans.push(Span::styled("  ", Style::default().fg(MUTED)));
                    continue;
                }

                // Dynamic multi-frequency phase calculation
                let phase1 = ((app.frame * 2 + (sm_idx as u64 * 7)) % 79) as f64 / 79.0;
                let phase2 = ((app.frame * 3 + (sm_idx as u64 * 13)) % 53) as f64 / 53.0;
                let twinkle = (phase1 * std::f64::consts::TAU).sin() * 0.18
                    + (phase2 * std::f64::consts::TAU).cos() * 0.12;

                // Sparkle flash on activity spike
                let sparkle_trigger =
                    (app.frame + sm_idx as u64 * 17).is_multiple_of(61) && params.activity > 0.08;

                let level = if sparkle_trigger {
                    4
                } else {
                    ((params.activity * 3.8 + phase1 * 0.45 + twinkle).floor() as usize).min(4)
                };

                let ch = if sparkle_trigger {
                    '✦'
                } else if params.tensor > 0.15 && (sm_idx % 4 == 0) {
                    '◈'
                } else {
                    GLYPHS[level]
                };

                // Dynamic psychedelic HSV color calculation
                let hue =
                    (params.temp_hue + (sm_idx as f64 * 3.2) + (app.frame as f64 * 1.5)) % 360.0;
                let sat = if sparkle_trigger {
                    0.4
                } else if params.activity > 0.05 {
                    0.95
                } else {
                    0.75
                };
                let val = if sparkle_trigger {
                    1.0
                } else {
                    (0.32 + params.activity * 0.55 + level as f64 * 0.06 + twinkle).clamp(0.2, 1.0)
                };

                let sm_color = if sparkle_trigger {
                    WHITE
                } else if params.tensor > 0.15 && (sm_idx % 4 == 0) {
                    GOLD
                } else {
                    hsv_to_rgb(hue, sat, val)
                };

                spans.push(Span::styled(
                    format!("{ch} "),
                    Style::default()
                        .fg(sm_color)
                        .add_modifier(if sparkle_trigger || level >= 3 {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ));
            }

            spans.push(Span::styled("] ", Style::default().fg(DARK_GRAY)));
        }

        lines.push(Line::from(spans));
    }

    let border_color = heat_color(params.temperature);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(format!(
                    " 2D SM Matrix · {display_units} Units ({clusters_per_row} Clusters/Row) "
                ))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color)),
        ),
        area,
    );
}

fn render_telemetry_deck(
    frame: &mut Frame<'_>,
    area: Rect,
    device: &DeviceSnapshot,
    activity: f64,
    tensor: f64,
    temperature: f64,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Min(4),
        ])
        .split(area);

    // Compute Activity Gauges
    let mut compute_lines = Vec::new();
    let sm_ratio = device.gpu_ratio().unwrap_or(0.0);
    compute_lines.push(Line::from(vec![
        Span::styled("SM Active    ", Style::default().fg(CYAN)),
        Span::styled(
            format!("{:>4.0}% ", sm_ratio * 100.0),
            Style::default()
                .fg(util_color(sm_ratio))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("[Baseline {:>3.0}%]", activity * 100.0),
            Style::default().fg(MUTED),
        ),
    ]));
    compute_lines.push(Line::from(vec![
        Span::styled("Tensor Pipe  ", Style::default().fg(PINK)),
        Span::styled(
            format!("{:>4.0}% ", tensor * 100.0),
            Style::default()
                .fg(if tensor > 0.0 { PINK } else { MUTED })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if tensor > 0.0 {
                "[Active ◈]"
            } else {
                "[Idle]"
            },
            Style::default().fg(if tensor > 0.0 { GOLD } else { MUTED }),
        ),
    ]));
    compute_lines.push(Line::from(vec![
        Span::styled("DRAM / MemCtrl", Style::default().fg(AMBER)),
        Span::styled(
            format!(
                " {:>4.0}%",
                device.memory_activity_ratio().unwrap_or(0.0) * 100.0
            ),
            Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
        ),
    ]));

    frame.render_widget(
        Paragraph::new(compute_lines).block(panel(" Pipeline Activity ", CYAN)),
        chunks[0],
    );

    // Clocks & Thermals HUD
    let sample = &device.sample;
    let sm_clock = sample.clocks.sm_clock_mhz.as_ref().map(|m| m.value);
    let mem_clock = sample.clocks.memory_clock_mhz.as_ref().map(|m| m.value);
    let power = sample.power.power_watts.as_ref().map(|m| m.value);
    let limit = sample.power.power_limit_watts.as_ref().map(|m| m.value);
    let fan = sample.thermals.fan_percent.as_ref().map(|m| m.value);

    let clocks_lines = vec![
        Line::from(vec![
            Span::styled("Clock SM  ", Style::default().fg(MUTED)),
            Span::styled(
                fmt_mhz(sm_clock),
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" · MEM "),
            Span::styled(fmt_mhz(mem_clock), Style::default().fg(PINK)),
            Span::raw(" · "),
            Span::styled(
                sample.clocks.performance_state.as_deref().unwrap_or("P?"),
                Style::default().fg(GOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Power     ", Style::default().fg(MUTED)),
            Span::styled(
                format!("{} / {}", fmt_watts(power), fmt_watts(limit)),
                Style::default().fg(GREEN),
            ),
        ]),
        Line::from(vec![
            Span::styled("Thermals  ", Style::default().fg(MUTED)),
            Span::styled(
                fmt_temp(Some(temperature)),
                Style::default()
                    .fg(heat_color(temperature))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" · Fan "),
            Span::styled(
                fmt_percent(fan.map(|f| f / 100.0)),
                Style::default().fg(CYAN),
            ),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(clocks_lines).block(panel(" Clocks & Power ", GREEN)),
        chunks[1],
    );

    // Active Processes Summary
    let mut proc_lines = Vec::new();
    if device.processes.is_empty() {
        proc_lines.push(Line::from(Span::styled(
            "No active compute processes",
            Style::default().fg(MUTED),
        )));
    } else {
        for proc in device.processes.iter().take(4) {
            proc_lines.push(Line::from(vec![
                Span::styled(format!("{:>6} ", proc.pid), Style::default().fg(GOLD)),
                Span::styled(
                    short_name(proc.command.as_deref().unwrap_or("unknown"), 12),
                    Style::default().fg(WHITE),
                ),
                Span::raw(" "),
                Span::styled(
                    fmt_bytes(proc.used_gpu_memory_bytes),
                    Style::default().fg(CYAN),
                ),
            ]));
        }
    }

    frame.render_widget(
        Paragraph::new(proc_lines).block(panel(
            &format!(" Processes ({}) ", device.processes.len()),
            PINK,
        )),
        chunks[2],
    );
}

fn render_bottom_horizon(
    frame: &mut Frame<'_>,
    area: Rect,
    device: &DeviceSnapshot,
    reported_sm_count: Option<usize>,
    display_units: usize,
) {
    let pcie_tx = device
        .sample
        .links
        .pcie_tx_bytes_per_second
        .as_ref()
        .map(|m| m.value);
    let pcie_rx = device
        .sample
        .links
        .pcie_rx_bytes_per_second
        .as_ref()
        .map(|m| m.value);

    let vram_used = fmt_bytes(device.sample.memory.used_bytes.as_ref().map(|m| m.value));
    let vram_total = fmt_bytes(device.sample.memory.total_bytes.as_ref().map(|m| m.value));

    let horizon_line = Line::from(vec![
        Span::styled(
            " VRAM ",
            Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("{vram_used} / {vram_total} ")),
        Span::styled("──(PCIe TX: ", Style::default().fg(MUTED)),
        Span::styled(fmt_rate(pcie_tx), Style::default().fg(CYAN)),
        Span::styled(" · RX: ", Style::default().fg(MUTED)),
        Span::styled(fmt_rate(pcie_rx), Style::default().fg(PINK)),
        Span::styled(")──> ", Style::default().fg(MUTED)),
        Span::styled("L2 Crossbar ", Style::default().fg(AMBER)),
        Span::styled("──> ", Style::default().fg(MUTED)),
        Span::styled(
            "SM Compute Array",
            Style::default().fg(NEON_GREEN).add_modifier(Modifier::BOLD),
        ),
    ]);

    let caption = format!(
        "Illustrative layout: all cells share device-level NVML activity. {}",
        reported_sm_count.map_or_else(
            || format!("{display_units} display cells; NVML did not expose physical SM count."),
            |count| format!(
                "Displaying {display_units}/{count} hardware SMs in structured 2D topology."
            )
        )
    );

    let lines = vec![
        horizon_line,
        Line::from(Span::styled(caption, Style::default().fg(MUTED))),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(panel(" Memory Horizon & Context ", MUTED)),
        area,
    );
}
