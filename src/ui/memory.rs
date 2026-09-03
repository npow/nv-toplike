// SPDX-License-Identifier: Apache-2.0

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::model::DeviceSnapshot;
use crate::ui::colors::*;
use crate::ui::{App, animated_link, fmt_bytes, fmt_mhz, fmt_percent, panel, short_name};

pub fn render_memory(frame: &mut Frame<'_>, area: Rect, app: &App, device: &DeviceSnapshot) {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(area);

    let sample = &device.sample;
    let used_bytes = sample.memory.used_bytes.as_ref().map(|m| m.value);
    let total_bytes = sample.memory.total_bytes.as_ref().map(|m| m.value);
    let mem_activity = device.memory_activity_ratio();
    let mem_clock = sample.clocks.memory_clock_mhz.as_ref().map(|m| m.value);

    // Top Header HUD
    let header_line = Line::from(vec![
        Span::styled(
            format!(" {} ", short_name(&device.device.name, 34)),
            Style::default()
                .fg(Color::Black)
                .bg(PINK)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  VRAM Allocation: "),
        Span::styled(
            format!("{} / {}", fmt_bytes(used_bytes), fmt_bytes(total_bytes)),
            Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                " ({:.1}%)",
                device.memory_fill_ratio().unwrap_or(0.0) * 100.0
            ),
            Style::default().fg(AMBER),
        ),
        Span::raw(" · Controller Activity: "),
        Span::styled(
            fmt_percent(mem_activity),
            Style::default()
                .fg(util_color(mem_activity.unwrap_or(0.0)))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" · MEM Clock: "),
        Span::styled(
            fmt_mhz(mem_clock),
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ),
    ]);

    frame.render_widget(
        Paragraph::new(header_line)
            .block(panel(" Memory Foundry · Hardware Hierarchy & Flow ", PINK)),
        main_layout[0],
    );

    // Split middle into Foundry Arena (left) and Telemetry Deck (right)
    let body_chunks = if main_layout[1].width >= 90 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(64), Constraint::Percentage(36)])
            .split(main_layout[1])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100), Constraint::Length(0)])
            .split(main_layout[1])
    };

    render_foundry_arena(frame, body_chunks[0], app, device);

    if body_chunks[1].width > 15 {
        render_memory_deck(frame, body_chunks[1], device);
    }

    // Bottom Semantics Footer
    frame.render_widget(
        Paragraph::new(
            "Truthful telemetry: PCIe rates and VRAM fill are direct NVML measurements. Internal L2/SM motion is architectural context.",
        )
        .style(Style::default().fg(MUTED))
        .alignment(Alignment::Center)
        .block(panel(" Hierarchy Semantics ", MUTED)),
        main_layout[2],
    );
}

fn render_foundry_arena(frame: &mut Frame<'_>, area: Rect, app: &App, device: &DeviceSnapshot) {
    let stages = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // PCIe Transport
            Constraint::Min(5),    // VRAM Reservoir
            Constraint::Length(4), // L2 Cache
            Constraint::Length(4), // SM Local Context
        ])
        .split(area);

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

    // Stage 1: PCIe Transport Highway
    let transport_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(22),
            Constraint::Percentage(56),
            Constraint::Percentage(22),
        ])
        .split(stages[0]);

    frame.render_widget(
        Paragraph::new("HOST RAM\n(CPU Realm)")
            .alignment(Alignment::Center)
            .style(Style::default().fg(WHITE))
            .block(panel(" Source / Sink ", MUTED)),
        transport_chunks[0],
    );

    let link_lines = vec![
        Line::from(animated_link("GPU TX (D2H)", pcie_tx, app.frame, true)),
        Line::from(animated_link(
            "GPU RX (H2D)",
            pcie_rx,
            app.frame.wrapping_add(7),
            false,
        )),
    ];
    frame.render_widget(
        Paragraph::new(link_lines)
            .alignment(Alignment::Center)
            .block(panel(" PCIe Bus Traffic (Measured Throughput) ", CYAN)),
        transport_chunks[1],
    );

    frame.render_widget(
        Paragraph::new("GPU VRAM\n(Framebuffer)")
            .alignment(Alignment::Center)
            .style(Style::default().fg(AMBER).add_modifier(Modifier::BOLD))
            .block(panel(" Framebuffer ", AMBER)),
        transport_chunks[2],
    );

    // Stage 2: VRAM Reservoir
    render_vram_reservoir(frame, stages[1], app, device);

    // Stage 3: L2 Cache Crossbar
    render_l2_cache_stage(frame, stages[2], app, device);

    // Stage 4: SM Local Execution Vaults
    render_sm_vaults_stage(frame, stages[3], app, device);
}

fn render_vram_reservoir(frame: &mut Frame<'_>, area: Rect, app: &App, device: &DeviceSnapshot) {
    let fill_ratio = device.memory_fill_ratio().unwrap_or(0.0);
    let mem_activity = device.memory_activity_ratio().unwrap_or(0.0);
    let inner_width = area.width.saturating_sub(4) as usize;
    let inner_height = area.height.saturating_sub(3) as usize;

    let mut lines = Vec::new();
    let filled_chars = ((inner_width as f64) * fill_ratio).round() as usize;

    for row in 0..inner_height.min(4) {
        let mut row_str = String::with_capacity(inner_width);
        for col in 0..inner_width {
            if col < filled_chars {
                let pulse = (app.frame.wrapping_add(col as u64 * 3 + row as u64 * 7))
                    .is_multiple_of(13)
                    && mem_activity > 0.05;
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
            Style::default().fg(if fill_ratio > 0.85 {
                RED
            } else if fill_ratio > 0.50 {
                AMBER
            } else {
                PINK
            }),
        )));
    }

    let used_fmt = fmt_bytes(device.sample.memory.used_bytes.as_ref().map(|m| m.value));
    let total_fmt = fmt_bytes(device.sample.memory.total_bytes.as_ref().map(|m| m.value));
    let free_fmt = fmt_bytes(device.sample.memory.free_bytes.as_ref().map(|m| m.value));

    lines.push(Line::from(vec![
        Span::styled("Reservoir: ", Style::default().fg(MUTED)),
        Span::styled(
            format!("{used_fmt} Allocated ({:.1}%) ", fill_ratio * 100.0),
            Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
        ),
        Span::raw("· Free: "),
        Span::styled(free_fmt, Style::default().fg(GREEN)),
        Span::raw(" · Capacity: "),
        Span::styled(total_fmt, Style::default().fg(WHITE)),
    ]));

    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(panel(" VRAM / HBM Memory Reservoir ", AMBER)),
        area,
    );
}

fn render_l2_cache_stage(frame: &mut Frame<'_>, area: Rect, app: &App, device: &DeviceSnapshot) {
    let activity = device.memory_activity_ratio().unwrap_or(0.0);
    let mut spans = Vec::new();
    let num_slices = 8;

    for i in 0..num_slices {
        let phase = ((app.frame + i as u64 * 5) % 11) as f64 / 11.0;
        let active = activity > 0.05 && phase > 0.3;
        let ch = if active { '◆' } else { '◇' };
        let col = if active { AMBER } else { DARK_GRAY };

        spans.push(Span::styled(
            format!("[L2_{i}:{ch}] "),
            Style::default().fg(col),
        ));
    }

    let lines = vec![
        Line::from(spans),
        Line::from(Span::styled(
            format!(
                "High-Speed L2 Crossbar Cache Partitions · Activity {:.0}%",
                activity * 100.0
            ),
            Style::default().fg(AMBER),
        )),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(panel(" L2 Cache Partitions & Crossbar ", AMBER)),
        area,
    );
}

fn render_sm_vaults_stage(frame: &mut Frame<'_>, area: Rect, app: &App, device: &DeviceSnapshot) {
    let visual_act = app.visual_activity(device);
    let mut spans = Vec::new();
    let num_vaults = 10;

    for i in 0..num_vaults {
        let phase = ((app.frame * 2 + i as u64 * 7) % 17) as f64 / 17.0;
        let level = ((visual_act * 4.0 + phase * 0.5).floor() as usize).min(4);
        let glyphs = ['·', '∘', '○', '◉', '●'];
        let ch = glyphs[level];
        let col = if level >= 3 {
            NEON_GREEN
        } else if level >= 1 {
            CYAN
        } else {
            MUTED
        };

        spans.push(Span::styled(
            format!("{ch} "),
            Style::default().fg(col).add_modifier(if level >= 3 {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
        ));
    }

    let lines = vec![
        Line::from(spans),
        Line::from(Span::styled(
            format!(
                "SM Execution Engine · Register Files & Shared L1 Memory · Load {:.0}%",
                visual_act * 100.0
            ),
            Style::default().fg(CYAN),
        )),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(panel(" SM Execution Vaults & L1 Context ", CYAN)),
        area,
    );
}

fn render_memory_deck(frame: &mut Frame<'_>, area: Rect, device: &DeviceSnapshot) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Min(4),
        ])
        .split(area);

    let sample = &device.sample;
    let used = sample.memory.used_bytes.as_ref().map(|m| m.value);
    let total = sample.memory.total_bytes.as_ref().map(|m| m.value);
    let free = sample.memory.free_bytes.as_ref().map(|m| m.value);
    let reserved = sample.memory.reserved_bytes.as_ref().map(|m| m.value);

    // Allocation Card
    let alloc_lines = vec![
        Line::from(vec![
            Span::styled("Allocated ", Style::default().fg(MUTED)),
            Span::styled(
                fmt_bytes(used),
                Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" / "),
            Span::styled(fmt_bytes(total), Style::default().fg(WHITE)),
        ]),
        Line::from(vec![
            Span::styled("Available ", Style::default().fg(MUTED)),
            Span::styled(fmt_bytes(free), Style::default().fg(GREEN)),
            Span::raw(" · Res: "),
            Span::styled(fmt_bytes(reserved), Style::default().fg(MUTED)),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(alloc_lines).block(panel(" VRAM Allocation ", AMBER)),
        chunks[0],
    );

    // Controller & Clocks
    let mem_clock = sample.clocks.sm_clock_mhz.as_ref().map(|m| m.value);
    let dram_act = device.memory_activity_ratio().unwrap_or(0.0);
    let ctrl_lines = vec![
        Line::from(vec![
            Span::styled("DRAM Active ", Style::default().fg(MUTED)),
            Span::styled(
                format!("{:.0}%", dram_act * 100.0),
                Style::default()
                    .fg(util_color(dram_act))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Memory Clock", Style::default().fg(MUTED)),
            Span::styled(format!(" {mem_clock:?} MHz"), Style::default().fg(CYAN)),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(ctrl_lines).block(panel(" Controller & Clocks ", PINK)),
        chunks[1],
    );

    // Reliability & ECC
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
    let retired_corr = sample
        .health
        .retired_pages_corrected
        .as_ref()
        .map(|m| m.value)
        .unwrap_or(0);
    let retired_uncorr = sample
        .health
        .retired_pages_uncorrected
        .as_ref()
        .map(|m| m.value)
        .unwrap_or(0);

    let ecc_lines = vec![
        Line::from(vec![
            Span::styled("ECC Volatile: ", Style::default().fg(MUTED)),
            Span::styled(format!("{ecc_vol} corr"), Style::default().fg(GREEN)),
            Span::raw(" · "),
            Span::styled(
                format!("{ecc_un} uncorr"),
                Style::default().fg(if ecc_un > 0 { RED } else { GREEN }),
            ),
        ]),
        Line::from(vec![
            Span::styled("Retired Pages:", Style::default().fg(MUTED)),
            Span::styled(
                format!(" {retired_corr} corr / {retired_uncorr} uncorr"),
                Style::default().fg(if retired_uncorr > 0 { RED } else { GREEN }),
            ),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(ecc_lines).block(panel(" Memory Health & ECC ", GREEN)),
        chunks[2],
    );
}
