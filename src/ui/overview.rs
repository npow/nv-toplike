// SPDX-License-Identifier: Apache-2.0

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, Wrap};

use crate::model::{DeviceSnapshot, ProcessKind, Snapshot};
use crate::ui::colors::*;
use crate::ui::{App, fmt_bytes, fmt_mhz, fmt_percent, fmt_rate, fmt_temp, fmt_watts, panel, render_gauge, short_name, short_uuid};

pub fn render_overview(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    snapshot: &Snapshot,
) {
    let device = &snapshot.devices[app.selected];
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Percentage(45),
            Constraint::Percentage(55),
        ])
        .split(area);

    render_device_selector(frame, layout[0], snapshot, app.selected);

    // Gauges Row
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
        " Power draw / limit ",
        device.power_ratio(),
        GREEN,
    );

    // Middle Telemetry Cards (3-column layout)
    let mid_chunks = if layout[2].width >= 100 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(34),
                Constraint::Percentage(33),
                Constraint::Percentage(33),
            ])
            .split(layout[2])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(layout[2])
    };

    render_hardware_card(frame, mid_chunks[0], device);
    render_clocks_card(frame, mid_chunks[1], device);
    if mid_chunks.len() > 2 {
        render_memory_engines_card(frame, mid_chunks[2], device);
    }

    // Lower Section: Processes & Diagnostics
    let lower_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(layout[3]);

    render_processes_table(frame, lower_chunks[0], device);
    render_diagnostics_card(frame, lower_chunks[1], device);
}

fn render_device_selector(
    frame: &mut Frame<'_>,
    area: Rect,
    snapshot: &Snapshot,
    selected: usize,
) {
    let spans = snapshot
        .devices
        .iter()
        .enumerate()
        .flat_map(|(index, device)| {
            let is_sel = index == selected;
            let temp = device.temperature_c().unwrap_or(0.0);
            let gpu = device.gpu_ratio().unwrap_or(0.0) * 100.0;
            let pwr = device.sample.power.power_watts.as_ref().map(|m| m.value).unwrap_or(0.0);

            let label = format!(
                " GPU {}: {} · {:.0}% · {:.0}°C · {:.0}W ",
                device.device.display_index.unwrap_or(index as u32),
                short_name(&device.device.name, 18),
                gpu,
                temp,
                pwr,
            );

            let style = if is_sel {
                Style::default()
                    .fg(Color::Black)
                    .bg(CYAN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };

            [Span::styled(label, style), Span::raw("  ")]
        })
        .collect::<Vec<_>>();

    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .wrap(Wrap { trim: true })
            .block(panel(" GPUs · UUID-stable selector (←/→) ", CYAN)),
        area,
    );
}

fn render_hardware_card(frame: &mut Frame<'_>, area: Rect, device: &DeviceSnapshot) {
    let sample = &device.sample;
    let sm_count = device.device.compute_units.map_or_else(|| "N/A".to_owned(), |c| format!("{c} SMs"));
    let pcie_tx = sample.links.pcie_tx_bytes_per_second.as_ref().map(|m| m.value);
    let pcie_rx = sample.links.pcie_rx_bytes_per_second.as_ref().map(|m| m.value);

    let lines = vec![
        Line::from(vec![
            Span::styled("Model ", Style::default().fg(MUTED)),
            Span::styled(&device.device.name, Style::default().fg(WHITE).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Arch  ", Style::default().fg(MUTED)),
            Span::styled(device.device.architecture.as_deref().unwrap_or("NVIDIA"), Style::default().fg(GOLD)),
            Span::raw(" · Compute: "),
            Span::styled(sm_count, Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("UUID  ", Style::default().fg(MUTED)),
            Span::styled(short_uuid(&device.device.id), Style::default().fg(WHITE)),
        ]),
        Line::from(vec![
            Span::styled("Bus   ", Style::default().fg(MUTED)),
            Span::styled(device.device.pci_bus_id.as_deref().unwrap_or("N/A"), Style::default().fg(WHITE)),
            Span::raw(" · PCIe Gen"),
            Span::styled(sample.links.pcie_generation.map_or_else(|| "?".to_owned(), |v| v.to_string()), Style::default().fg(GREEN)),
            Span::raw(" x"),
            Span::styled(sample.links.pcie_width.map_or_else(|| "?".to_owned(), |v| v.to_string()), Style::default().fg(GREEN)),
        ]),
        Line::from(vec![
            Span::styled("Link  ", Style::default().fg(MUTED)),
            Span::styled("TX: ", Style::default().fg(MUTED)),
            Span::styled(format!("{:>10}", fmt_rate(pcie_tx)), Style::default().fg(CYAN)),
            Span::styled(" · RX: ", Style::default().fg(MUTED)),
            Span::styled(format!("{:>10}", fmt_rate(pcie_rx)), Style::default().fg(PINK)),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(panel(" Architecture & Bus ", CYAN)),
        area,
    );
}

fn render_clocks_card(frame: &mut Frame<'_>, area: Rect, device: &DeviceSnapshot) {
    let sample = &device.sample;
    let sm_clock = sample.clocks.sm_clock_mhz.as_ref().map(|m| m.value);
    let mem_clock = sample.clocks.memory_clock_mhz.as_ref().map(|m| m.value);
    let vid_clock = sample.clocks.video_clock_mhz.as_ref().map(|m| m.value);
    let power = sample.power.power_watts.as_ref().map(|m| m.value);
    let limit = sample.power.power_limit_watts.as_ref().map(|m| m.value);
    let temp = device.temperature_c();
    let slowdown = sample.thermals.slowdown_threshold_celsius.as_ref().map(|m| m.value);
    let fan = sample.thermals.fan_percent.as_ref().map(|m| m.value);

    let lines = vec![
        Line::from(vec![
            Span::styled("Clocks  ", Style::default().fg(MUTED)),
            Span::styled("SM ", Style::default().fg(MUTED)),
            Span::styled(fmt_mhz(sm_clock), Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(" · MEM ", Style::default().fg(MUTED)),
            Span::styled(fmt_mhz(mem_clock), Style::default().fg(PINK)),
            Span::styled(" · VID ", Style::default().fg(MUTED)),
            Span::styled(fmt_mhz(vid_clock), Style::default().fg(AMBER)),
        ]),
        Line::from(vec![
            Span::styled("P-State ", Style::default().fg(MUTED)),
            Span::styled(sample.clocks.performance_state.as_deref().unwrap_or("P?"), Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
            Span::raw(" · Power: "),
            Span::styled(fmt_watts(power), Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
            Span::raw(" / "),
            Span::styled(fmt_watts(limit), Style::default().fg(MUTED)),
        ]),
        Line::from(vec![
            Span::styled("Temp    ", Style::default().fg(MUTED)),
            Span::styled(fmt_temp(temp), Style::default().fg(heat_color(temp.unwrap_or(35.0))).add_modifier(Modifier::BOLD)),
            Span::raw(" (Slowdown: "),
            Span::styled(fmt_temp(slowdown), Style::default().fg(MUTED)),
            Span::raw(") · Fan: "),
            Span::styled(fmt_percent(fan.map(|f| f / 100.0)), Style::default().fg(CYAN)),
        ]),
        Line::from(vec![
            Span::styled("Energy  ", Style::default().fg(MUTED)),
            Span::styled(
                sample.power.energy_millijoules.as_ref().map_or_else(|| "N/A".to_owned(), |m| format!("{:.2} MJ", m.value as f64 / 1_000_000.0)),
                Style::default().fg(GREEN),
            ),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(panel(" Clocks & Power ", GREEN)),
        area,
    );
}

fn render_memory_engines_card(frame: &mut Frame<'_>, area: Rect, device: &DeviceSnapshot) {
    let sample = &device.sample;
    let used = sample.memory.used_bytes.as_ref().map(|m| m.value);
    let total = sample.memory.total_bytes.as_ref().map(|m| m.value);
    let free = sample.memory.free_bytes.as_ref().map(|m| m.value);
    let reserved = sample.memory.reserved_bytes.as_ref().map(|m| m.value);

    let enc = sample.utilization.encoder_ratio.as_ref().map(|m| m.value);
    let dec = sample.utilization.decoder_ratio.as_ref().map(|m| m.value);
    let replay = sample.health.pcie_replay_counter.as_ref().map(|m| m.value).unwrap_or(0);

    // Visual mini bar for memory
    let fill_ratio = device.memory_fill_ratio().unwrap_or(0.0);
    let bar_width = 16;
    let filled = (fill_ratio * bar_width as f64).round() as usize;
    let mut bar = String::new();
    for i in 0..bar_width {
        if i < filled {
            bar.push('▓');
        } else {
            bar.push('░');
        }
    }

    let lines = vec![
        Line::from(vec![
            Span::styled("VRAM  ", Style::default().fg(MUTED)),
            Span::styled(fmt_bytes(used), Style::default().fg(AMBER).add_modifier(Modifier::BOLD)),
            Span::raw(" / "),
            Span::styled(fmt_bytes(total), Style::default().fg(WHITE)),
            Span::raw(" (Free: "),
            Span::styled(fmt_bytes(free), Style::default().fg(GREEN)),
            Span::raw(")"),
        ]),
        Line::from(vec![
            Span::styled("Pool  ", Style::default().fg(MUTED)),
            Span::styled(bar, Style::default().fg(AMBER)),
            Span::styled(format!(" {:.1}%", fill_ratio * 100.0), Style::default().fg(AMBER)),
            Span::raw(" · Res: "),
            Span::styled(fmt_bytes(reserved), Style::default().fg(MUTED)),
        ]),
        Line::from(vec![
            Span::styled("Video ", Style::default().fg(MUTED)),
            Span::styled("Encoder: ", Style::default().fg(MUTED)),
            Span::styled(fmt_percent(enc), Style::default().fg(CYAN)),
            Span::styled(" · Decoder: ", Style::default().fg(MUTED)),
            Span::styled(fmt_percent(dec), Style::default().fg(PINK)),
        ]),
        Line::from(vec![
            Span::styled("Bus   ", Style::default().fg(MUTED)),
            Span::styled(format!("PCIe Replays: {replay}"), Style::default().fg(if replay > 0 { RED } else { GREEN })),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(panel(" VRAM & Engines ", AMBER)),
        area,
    );
}

fn render_processes_table(frame: &mut Frame<'_>, area: Rect, device: &DeviceSnapshot) {
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

    let title = format!(" Active Processes · {} ", device.processes.len());
    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Length(5),
            Constraint::Min(14),
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

fn render_diagnostics_card(frame: &mut Frame<'_>, area: Rect, device: &DeviceSnapshot) {
    let sample = &device.sample;
    let mut lines = Vec::new();

    let has_fault = sample
        .health
        .observations
        .iter()
        .any(|v| v.contains("uncorrected") || v.contains("slowdown") || v.contains("brake"));

    let status_badge = if has_fault {
        Span::styled(
            "● WARN / THROTTLE ACTIVE",
            Style::default().fg(RED).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            "● NOMINAL (No Observed Faults)",
            Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
        )
    };
    lines.push(Line::from(vec![
        Span::styled("Health Status: ", Style::default().fg(MUTED)),
        status_badge,
    ]));

    for obs in &sample.health.observations {
        lines.push(Line::from(vec![
            Span::styled("  ▪ ", Style::default().fg(MUTED)),
            Span::styled(
                obs,
                Style::default().fg(if obs.contains("uncorrected") || obs.contains("slowdown") {
                    RED
                } else {
                    GREEN
                }),
            ),
        ]));
    }

    if !sample.clocks.throttle_reasons.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Throttle: ", Style::default().fg(AMBER)),
            Span::styled(
                sample.clocks.throttle_reasons.join(", "),
                Style::default().fg(AMBER),
            ),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Throttle: ", Style::default().fg(MUTED)),
            Span::styled("None (Unconstrained)", Style::default().fg(GREEN)),
        ]));
    }

    let ecc_vol = sample
        .health
        .corrected_ecc_volatile
        .as_ref()
        .map(|m| m.value)
        .unwrap_or(0);
    let ecc_un = sample
        .health
        .uncorrected_ecc_volatile
        .as_ref()
        .map(|m| m.value)
        .unwrap_or(0);
    lines.push(Line::from(vec![
        Span::styled("ECC Volatile: ", Style::default().fg(MUTED)),
        Span::styled(format!("{ecc_vol} corr"), Style::default().fg(GREEN)),
        Span::raw(" · "),
        Span::styled(
            format!("{ecc_un} uncorr"),
            Style::default().fg(if ecc_un > 0 { RED } else { GREEN }),
        ),
    ]));

    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(panel(
            " Health & Diagnostics ",
            if has_fault { RED } else { GREEN },
        )),
        area,
    );
}
