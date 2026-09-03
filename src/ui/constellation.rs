// SPDX-License-Identifier: Apache-2.0

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::model::DeviceSnapshot;
use crate::ui::colors::*;
use crate::ui::{
    App, animated_link, fmt_bytes, fmt_mhz, fmt_percent, fmt_temp, fmt_watts, panel, short_name,
};

const GLYPHS: [char; 5] = ['·', '∘', '○', '◉', '●'];

struct SmGridParams {
    display_units: usize,
    activity: f64,
    tensor: f64,
    temp_hue: f64,
    temperature: f64,
}

pub fn render_constellation(frame: &mut Frame<'_>, area: Rect, app: &App, device: &DeviceSnapshot) {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),      // Top Header HUD
            Constraint::Percentage(58), // Upper: 2D SM Constellation & Pipeline Telemetry
            Constraint::Percentage(42), // Lower: Memory Reservoir & PCIe DMA Highway
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

    // 1. Top Header HUD
    render_header_hud(
        frame,
        main_layout[0],
        device,
        reported_sm_count,
        display_units,
        activity,
        tensor,
    );

    // Determine sidebar width based on total terminal width (38..48 cols)
    let right_width = if main_layout[1].width >= 80 {
        (main_layout[1].width * 38 / 100).clamp(46, 54)
    } else {
        0
    };

    // 2. Upper Section: SM Constellation Grid (Left) + Pipeline Activity Deck (Right)
    let upper_chunks = if right_width > 0 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(36), Constraint::Length(right_width)])
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
        temp_hue,
        temperature,
    };
    render_sm_grid(frame, upper_chunks[0], app, &params);

    if upper_chunks[1].width > 15 {
        render_pipeline_telemetry_deck(
            frame,
            upper_chunks[1],
            device,
            activity,
            tensor,
            temperature,
        );
    }

    // 3. Lower Section: VRAM & L2 Foundry (Left) + PCIe DMA Highway & Processes (Right)
    let lower_chunks = if right_width > 0 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(36), Constraint::Length(right_width)])
            .split(main_layout[2])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100), Constraint::Length(0)])
            .split(main_layout[2])
    };

    render_memory_foundry_card(frame, lower_chunks[0], app, device);

    if lower_chunks[1].width > 15 {
        render_pcie_processes_card(frame, lower_chunks[1], app, device);
    }
}

fn render_header_hud(
    frame: &mut Frame<'_>,
    area: Rect,
    device: &DeviceSnapshot,
    reported_sm_count: Option<usize>,
    display_units: usize,
    activity: f64,
    tensor: f64,
) {
    let sm_label = reported_sm_count.map_or_else(
        || format!("{display_units} SMs"),
        |count| format!("{count} SMs"),
    );

    let header_line = Line::from(vec![
        Span::styled(
            format!(" {} ", short_name(&device.device.name, 32)),
            Style::default()
                .fg(Color::Black)
                .bg(CYAN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            sm_label,
            Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · Arch: ", Style::default().fg(MUTED)),
        Span::styled(
            device.device.architecture.as_deref().unwrap_or("NVIDIA"),
            Style::default().fg(WHITE),
        ),
        Span::styled(" · GPU Load: ", Style::default().fg(MUTED)),
        Span::styled(
            fmt_percent(device.gpu_ratio()),
            Style::default()
                .fg(util_color(device.gpu_ratio().unwrap_or(0.0)))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · Baseline Activity: ", Style::default().fg(MUTED)),
        Span::styled(
            format!("{:.0}%", activity * 100.0),
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · Tensor Pipe: ", Style::default().fg(MUTED)),
        Span::styled(
            if tensor > 0.0 {
                format!("{:.0}% ◈", tensor * 100.0)
            } else {
                "Idle".to_owned()
            },
            Style::default()
                .fg(if tensor > 0.0 { PINK } else { MUTED })
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    frame.render_widget(
        Paragraph::new(header_line).block(panel(" SM Constellation · Compute Array ", CYAN)),
        area,
    );
}

fn render_sm_grid(frame: &mut Frame<'_>, area: Rect, app: &App, params: &SmGridParams) {
    let inner_width = area.width.saturating_sub(4) as usize;
    let inner_height = area.height.saturating_sub(2) as usize;

    let cluster_size = if params.display_units > 48 {
        4
    } else if params.display_units > 16 {
        2
    } else {
        1
    };
    let cluster_char_len = 2 + cluster_size * 2 + 2; // e.g. "[ · · · · ] " = 12

    let total_clusters = params.display_units.div_ceil(cluster_size);
    let prefix_len = 7; // "SM000│ "
    let max_clusters_per_row =
        (inner_width.saturating_sub(prefix_len + 1) / cluster_char_len).max(1);

    // Expand clusters per row to use the available width fully
    let clusters_per_row = max_clusters_per_row.min(total_clusters).max(1);
    let total_rows = total_clusters.div_ceil(clusters_per_row);

    let mut lines = Vec::new();

    for row_idx in 0..total_rows.min(inner_height) {
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
                if sm_idx >= params.display_units {
                    spans.push(Span::styled("  ", Style::default().fg(MUTED)));
                    continue;
                }

                let phase1 = ((app.frame * 2 + (sm_idx as u64 * 7)) % 79) as f64 / 79.0;
                let phase2 = ((app.frame * 3 + (sm_idx as u64 * 13)) % 53) as f64 / 53.0;
                let twinkle = (phase1 * std::f64::consts::TAU).sin() * 0.18
                    + (phase2 * std::f64::consts::TAU).cos() * 0.12;

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

                let hue =
                    (params.temp_hue + (sm_idx as f64 * 3.2) + (app.frame as f64 * 1.5)) % 360.0;
                let sat = if sparkle_trigger {
                    0.35
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

                let color = if sparkle_trigger {
                    WHITE
                } else if params.tensor > 0.15 && (sm_idx % 4 == 0) {
                    GOLD
                } else {
                    hsv_to_rgb(hue, sat, val)
                };

                spans.push(Span::styled(
                    format!("{ch} "),
                    Style::default()
                        .fg(color)
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
                    " 2D SM Matrix · {} Units ({clusters_per_row} Clusters/Row) ",
                    params.display_units
                ))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color)),
        ),
        area,
    );
}

fn render_pipeline_telemetry_deck(
    frame: &mut Frame<'_>,
    area: Rect,
    device: &DeviceSnapshot,
    activity: f64,
    tensor: f64,
    temperature: f64,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // 1. Pipeline Gauges & Load
    let sm_ratio = device.gpu_ratio().unwrap_or(0.0);
    let mem_act = device.memory_activity_ratio().unwrap_or(0.0);
    let enc = device
        .sample
        .utilization
        .encoder_ratio
        .as_ref()
        .map(|m| m.value)
        .unwrap_or(0.0);
    let dec = device
        .sample
        .utilization
        .decoder_ratio
        .as_ref()
        .map(|m| m.value)
        .unwrap_or(0.0);

    let pipe_lines = vec![
        Line::from(vec![
            Span::styled("SM Compute  ", Style::default().fg(CYAN)),
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
        ]),
        Line::from(vec![
            Span::styled("Tensor Core ", Style::default().fg(PINK)),
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
        ]),
        Line::from(vec![
            Span::styled("DRAM Ctrl   ", Style::default().fg(AMBER)),
            Span::styled(
                format!("{:>4.0}% ", mem_act * 100.0),
                Style::default()
                    .fg(util_color(mem_act))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if mem_act > 0.1 {
                    "[Active ◆]"
                } else {
                    "[Idle]"
                },
                Style::default().fg(AMBER),
            ),
        ]),
        Line::from(vec![
            Span::styled("Video Engine", Style::default().fg(WHITE)),
            Span::styled(
                format!(" ENC: {:>3.0}% · DEC: {:>3.0}%", enc * 100.0, dec * 100.0),
                Style::default().fg(MUTED),
            ),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(pipe_lines).block(panel(" Pipeline & Engine Activity ", CYAN)),
        chunks[0],
    );

    // 2. Clocks, Power & Thermals
    let sample = &device.sample;
    let sm_clk = sample.clocks.sm_clock_mhz.as_ref().map(|m| m.value);
    let mem_clk = sample.clocks.memory_clock_mhz.as_ref().map(|m| m.value);
    let power = sample.power.power_watts.as_ref().map(|m| m.value);
    let limit = sample.power.power_limit_watts.as_ref().map(|m| m.value);
    let fan = sample.thermals.fan_percent.as_ref().map(|m| m.value);

    let clocks_lines = vec![
        Line::from(vec![
            Span::styled("Clocks  ", Style::default().fg(MUTED)),
            Span::styled(
                fmt_mhz(sm_clk),
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" · MEM "),
            Span::styled(fmt_mhz(mem_clk), Style::default().fg(PINK)),
            Span::raw(" · "),
            Span::styled(
                sample.clocks.performance_state.as_deref().unwrap_or("P?"),
                Style::default().fg(GOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Power   ", Style::default().fg(MUTED)),
            Span::styled(
                format!("{} / {}", fmt_watts(power), fmt_watts(limit)),
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Thermal ", Style::default().fg(MUTED)),
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
        Paragraph::new(clocks_lines).block(panel(" Clocks & Power Dynamics ", GREEN)),
        chunks[1],
    );
}

fn render_memory_foundry_card(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    device: &DeviceSnapshot,
) {
    let fill_ratio = device.memory_fill_ratio().unwrap_or(0.0);
    let mem_act = device.memory_activity_ratio().unwrap_or(0.0);
    let inner_width = area.width.saturating_sub(4) as usize;
    let inner_height = area.height.saturating_sub(2) as usize;

    let mut lines = Vec::new();

    // 1. Calculate L2 slice arrangement
    let num_slices: usize = 16;
    let slices_per_row = (inner_width / 10).clamp(4, 8);
    let l2_rows = num_slices.div_ceil(slices_per_row);

    // 2. Scale VRAM Reservoir block rows to fill the remaining height exactly
    let bar_rows = inner_height.saturating_sub(1 + l2_rows).max(2);
    let filled_total = ((inner_width as f64) * fill_ratio).round() as usize;

    for row in 0..bar_rows {
        let mut row_str = String::with_capacity(inner_width);
        for col in 0..inner_width {
            if col < filled_total {
                let pulse = (app.frame.wrapping_add(col as u64 * 5 + row as u64 * 11))
                    .is_multiple_of(13)
                    && mem_act > 0.05;
                if pulse {
                    row_str.push('◆');
                } else {
                    row_str.push('▓');
                }
            } else {
                row_str.push('░');
            }
        }
        lines.push(Line::from(Span::styled(
            row_str,
            Style::default().fg(AMBER),
        )));
    }

    let used_fmt = fmt_bytes(device.sample.memory.used_bytes.as_ref().map(|m| m.value));
    let tot_fmt = fmt_bytes(device.sample.memory.total_bytes.as_ref().map(|m| m.value));
    let free_fmt = fmt_bytes(device.sample.memory.free_bytes.as_ref().map(|m| m.value));
    lines.push(Line::from(vec![
        Span::styled(
            format!("VRAM: {used_fmt} / {tot_fmt} ({:.1}%)", fill_ratio * 100.0),
            Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" · Free: "),
        Span::styled(free_fmt, Style::default().fg(GREEN)),
    ]));

    // 3. Render L2 Cache Crossbar Slices across rows
    for r in 0..l2_rows {
        let mut l2_spans = Vec::new();
        for c in 0..slices_per_row {
            let i = r * slices_per_row + c;
            if i >= num_slices {
                break;
            }
            let phase = ((app.frame + i as u64 * 7) % 11) as f64 / 11.0;
            let active = mem_act > 0.05 && phase > 0.3;
            let ch = if active { '◆' } else { '◇' };
            let col = if active { AMBER } else { DARK_GRAY };
            l2_spans.push(Span::styled(
                format!("[L2_{i:02}:{ch}] "),
                Style::default().fg(col),
            ));
        }
        lines.push(Line::from(l2_spans));
    }

    frame.render_widget(
        Paragraph::new(lines).block(panel(
            " Memory Foundry · VRAM Reservoir & L2 Crossbar ",
            AMBER,
        )),
        area,
    );
}

fn render_pcie_processes_card(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    device: &DeviceSnapshot,
) {
    let sample = &device.sample;
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
    let inner_height = area.height.saturating_sub(2) as usize;

    let mut lines = Vec::new();

    // 1. PCIe Interconnect Links
    lines.push(Line::from(animated_link(
        "Host TX (D2H)",
        pcie_tx,
        app.frame,
        true,
    )));
    lines.push(Line::from(animated_link(
        "Host RX (H2D)",
        pcie_rx,
        app.frame.wrapping_add(7),
        false,
    )));

    // 2. PCIe Link Speed & Replays
    let gen_str = sample
        .links
        .pcie_generation
        .map_or_else(|| "?".to_owned(), |v| v.to_string());
    let width_str = sample
        .links
        .pcie_width
        .map_or_else(|| "?".to_owned(), |v| v.to_string());
    let replays = sample
        .health
        .pcie_replay_counter
        .as_ref()
        .map(|m| m.value)
        .unwrap_or(0);
    lines.push(Line::from(vec![
        Span::styled("Bus Link: ", Style::default().fg(MUTED)),
        Span::styled(
            format!("PCIe Gen{gen_str} x{width_str}"),
            Style::default().fg(GREEN),
        ),
        Span::raw(" · Replays: "),
        Span::styled(
            format!("{replays}"),
            Style::default().fg(if replays > 0 { RED } else { GREEN }),
        ),
    ]));

    // 3. Active Processes Table or Reliability Diagnostics
    let max_procs = inner_height.saturating_sub(4).max(1);
    if device.processes.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Processes: ", Style::default().fg(MUTED)),
            Span::styled("None active (Idle)", Style::default().fg(CYAN)),
        ]));
        // Add reliability status lines to fill the space
        let ecc_corr = sample
            .health
            .corrected_ecc_volatile
            .as_ref()
            .map(|m| m.value)
            .unwrap_or(0);
        let ecc_uncorr = sample
            .health
            .uncorrected_ecc_volatile
            .as_ref()
            .map(|m| m.value)
            .unwrap_or(0);
        lines.push(Line::from(vec![
            Span::styled("ECC Status: ", Style::default().fg(MUTED)),
            Span::styled(format!("{ecc_corr} corrected"), Style::default().fg(GREEN)),
            Span::raw(" · "),
            Span::styled(
                format!("{ecc_uncorr} uncorrected"),
                Style::default().fg(if ecc_uncorr > 0 { RED } else { GREEN }),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Throttle State: ", Style::default().fg(MUTED)),
            Span::styled(
                if sample.clocks.throttle_reasons.is_empty() {
                    "Unconstrained (Nominal)"
                } else {
                    "Active"
                },
                Style::default().fg(if sample.clocks.throttle_reasons.is_empty() {
                    GREEN
                } else {
                    AMBER
                }),
            ),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "Active Compute Processes:",
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        )));
        for proc in device.processes.iter().take(max_procs) {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:>6} ", proc.pid), Style::default().fg(GOLD)),
                Span::styled(
                    short_name(proc.command.as_deref().unwrap_or("?"), 16),
                    Style::default().fg(WHITE),
                ),
                Span::raw(" · "),
                Span::styled(
                    fmt_bytes(proc.used_gpu_memory_bytes),
                    Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
                ),
            ]));
        }
    }

    frame.render_widget(
        Paragraph::new(lines).block(panel(" PCIe DMA Highway & Process Attribution ", CYAN)),
        area,
    );
}
