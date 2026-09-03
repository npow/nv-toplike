// SPDX-License-Identifier: Apache-2.0

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::model::DeviceSnapshot;
use crate::ui::colors::*;
use crate::ui::{
    App, animated_link, fmt_bytes, fmt_mhz, fmt_percent, panel, short_name,
};

pub fn render_memory(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    device: &DeviceSnapshot,
) {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(3),
        ])
        .split(area);

    let sample = &device.sample;
    let used_bytes = sample.memory.used_bytes.as_ref().map(|m| m.value);
    let total_bytes = sample.memory.total_bytes.as_ref().map(|m| m.value);
    let mem_activity = device.memory_activity_ratio();
    let mem_clock = sample.clocks.memory_clock_mhz.as_ref().map(|m| m.value);
    let fill_ratio = device.memory_fill_ratio().unwrap_or(0.0);

    // Top Header HUD
    let header_line = Line::from(vec![
        Span::styled(
            format!(" {} ", short_name(&device.device.name, 32)),
            Style::default().fg(Color::Black).bg(PINK).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  VRAM: "),
        Span::styled(
            format!("{} / {}", fmt_bytes(used_bytes), fmt_bytes(total_bytes)),
            Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" ({:.1}%)", fill_ratio * 100.0),
            Style::default().fg(AMBER),
        ),
        Span::raw(" · DRAM Controller Activity: "),
        Span::styled(
            fmt_percent(mem_activity),
            Style::default().fg(util_color(mem_activity.unwrap_or(0.0))).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" · MEM Clock: "),
        Span::styled(fmt_mhz(mem_clock), Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
    ]);

    frame.render_widget(
        Paragraph::new(header_line)
            .block(panel(" Memory Foundry · Hardware Memory Hierarchy & Allocation Flow ", PINK)),
        main_layout[0],
    );

    // Split middle into Foundry Arena (left) and Telemetry Deck (right)
    let body_chunks = if main_layout[1].width >= 96 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(main_layout[1])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100), Constraint::Length(0)])
            .split(main_layout[1])
    };

    render_foundry_canvas(frame, body_chunks[0], app, device);

    if body_chunks[1].width > 15 {
        render_memory_telemetry_deck(frame, body_chunks[1], device);
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

fn render_foundry_canvas(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    device: &DeviceSnapshot,
) {
    let stages = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Level 1: PCIe Transport
            Constraint::Percentage(40),    // Level 2: 2D VRAM Reservoir Map
            Constraint::Percentage(30), // Level 3: L2 Crossbar & Cache Slices
            Constraint::Percentage(30), // Level 4: SM Local Vaults
        ])
        .split(area);

    let sample = &device.sample;
    let pcie_tx = sample.links.pcie_tx_bytes_per_second.as_ref().map(|m| m.value);
    let pcie_rx = sample.links.pcie_rx_bytes_per_second.as_ref().map(|m| m.value);

    // Level 1: PCIe Transport Highway
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
        Line::from(animated_link("GPU RX (H2D)", pcie_rx, app.frame.wrapping_add(7), false)),
    ];
    frame.render_widget(
        Paragraph::new(link_lines)
            .alignment(Alignment::Center)
            .block(panel(" PCIe Interconnect (Measured Throughput) ", CYAN)),
        transport_chunks[1],
    );

    frame.render_widget(
        Paragraph::new("GPU VRAM\n(Framebuffer)")
            .alignment(Alignment::Center)
            .style(Style::default().fg(AMBER).add_modifier(Modifier::BOLD))
            .block(panel(" Framebuffer ", AMBER)),
        transport_chunks[2],
    );

    // Level 2: 2D VRAM Reservoir Block Map
    render_vram_block_map(frame, stages[1], app, device);

    // Level 3: L2 Cache Crossbar Slices Matrix
    render_l2_cache_matrix(frame, stages[2], app, device);

    // Level 4: SM Execution Vaults & Local Memory
    render_sm_vaults_matrix(frame, stages[3], app, device);
}

fn render_vram_block_map(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    device: &DeviceSnapshot,
) {
    let fill_ratio = device.memory_fill_ratio().unwrap_or(0.0);
    let mem_activity = device.memory_activity_ratio().unwrap_or(0.0);
    let inner_width = area.width.saturating_sub(4) as usize;
    let inner_height = area.height.saturating_sub(3) as usize;

    let mut lines = Vec::new();
    let total_cells = inner_width * inner_height.max(1);
    let filled_total = ((total_cells as f64) * fill_ratio).round() as usize;

    for row in 0..inner_height {
        let mut row_spans = Vec::new();
        let row_start = row * inner_width;

        for col in 0..inner_width {
            let cell_idx = row_start + col;
            if cell_idx < filled_total {
                let pulse = (app.frame.wrapping_add(cell_idx as u64 * 7)) .is_multiple_of(11) && mem_activity > 0.05;
                if pulse {
                    row_spans.push(Span::styled("◆", Style::default().fg(WHITE).add_modifier(Modifier::BOLD)));
                } else {
                    row_spans.push(Span::styled("▓", Style::default().fg(AMBER)));
                }
            } else {
                row_spans.push(Span::styled("░", Style::default().fg(DARK_GRAY)));
            }
        }
        lines.push(Line::from(row_spans));
    }

    let used_fmt = fmt_bytes(device.sample.memory.used_bytes.as_ref().map(|m| m.value));
    let total_fmt = fmt_bytes(device.sample.memory.total_bytes.as_ref().map(|m| m.value));
    let free_fmt = fmt_bytes(device.sample.memory.free_bytes.as_ref().map(|m| m.value));
    let res_fmt = fmt_bytes(device.sample.memory.reserved_bytes.as_ref().map(|m| m.value));

    lines.push(Line::from(vec![
        Span::styled("Memory Map: ", Style::default().fg(MUTED)),
        Span::styled(format!("{used_fmt} Used ({:.1}%)", fill_ratio * 100.0), Style::default().fg(AMBER).add_modifier(Modifier::BOLD)),
        Span::raw(" │ Free: "),
        Span::styled(free_fmt, Style::default().fg(GREEN)),
        Span::raw(" │ Reserved: "),
        Span::styled(res_fmt, Style::default().fg(MUTED)),
        Span::raw(" │ Total: "),
        Span::styled(total_fmt, Style::default().fg(WHITE)),
    ]));

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" 2D VRAM Memory Allocation Reservoir ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(AMBER)),
            ),
        area,
    );
}

fn render_l2_cache_matrix(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    device: &DeviceSnapshot,
) {
    let activity = device.memory_activity_ratio().unwrap_or(0.0);
    let inner_width = area.width.saturating_sub(4) as usize;
    let num_slices: usize = 16;
    let slice_width = 10; // "[L2_00: ◆] " = 11 chars
    let slices_per_row = (inner_width / slice_width).clamp(4, 8);

    let mut lines = Vec::new();
    let num_rows = num_slices.div_ceil(slices_per_row);

    for r in 0..num_rows {
        let mut spans = Vec::new();
        for c in 0..slices_per_row {
            let i = r * slices_per_row + c;
            if i >= num_slices {
                break;
            }
            let phase = ((app.frame + i as u64 * 7) % 13) as f64 / 13.0;
            let active = activity > 0.03 && phase > 0.25;
            let ch = if active { '◆' } else { '◇' };
            let col = if active { AMBER } else { DARK_GRAY };

            spans.push(Span::styled(format!("[L2_{i:02}:{ch}] "), Style::default().fg(col)));
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(Span::styled(
        format!("L2 Cache Crossbar Array (16 Partitions) · Crossbar Bus Activity {:.0}%", activity * 100.0),
        Style::default().fg(AMBER),
    )));

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" L2 Cache Partitions & Crossbar Fabric ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(AMBER)),
            ),
        area,
    );
}

fn render_sm_vaults_matrix(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    device: &DeviceSnapshot,
) {
    let visual_act = app.visual_activity(device);
    let inner_width = area.width.saturating_sub(4) as usize;
    let num_clusters: usize = 8;
    let cluster_width = 18;
    let clusters_per_row = (inner_width / cluster_width).clamp(2, 4);

    let mut lines = Vec::new();
    let num_rows = num_clusters.div_ceil(clusters_per_row);

    for r in 0..num_rows {
        let mut spans = Vec::new();
        for c in 0..clusters_per_row {
            let i = r * clusters_per_row + c;
            if i >= num_clusters {
                break;
            }
            let phase = ((app.frame * 2 + i as u64 * 11) % 19) as f64 / 19.0;
            let level = ((visual_act * 4.0 + phase * 0.45).floor() as usize).min(4);
            let glyphs = ['·', '∘', '○', '◉', '●'];
            let l1_ch = glyphs[level];
            let reg_ch = glyphs[((level as f64 * 0.8 + phase * 0.3).floor() as usize).min(4)];

            let col = if level >= 3 { NEON_GREEN } else if level >= 1 { CYAN } else { MUTED };

            spans.push(Span::styled(
                format!("[C{i} L1:{l1_ch} Reg:{reg_ch}]  "),
                Style::default().fg(col),
            ));
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(Span::styled(
        format!("SM Execution Vaults · L1 Shared Memory & Register Files · Activity {:.0}%", visual_act * 100.0),
        Style::default().fg(CYAN),
    )));

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" SM Compute Vaults & L1 / Shared Context ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(CYAN)),
            ),
        area,
    );
}

fn render_memory_telemetry_deck(
    frame: &mut Frame<'_>,
    area: Rect,
    device: &DeviceSnapshot,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
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
            Span::styled(fmt_bytes(used), Style::default().fg(AMBER).add_modifier(Modifier::BOLD)),
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
        Paragraph::new(alloc_lines)
            .block(panel(" VRAM Allocation ", AMBER)),
        chunks[0],
    );

    // Controller & Clocks
    let mem_clock = sample.clocks.memory_clock_mhz.as_ref().map(|m| m.value);
    let dram_act = device.memory_activity_ratio().unwrap_or(0.0);
    let ctrl_lines = vec![
        Line::from(vec![
            Span::styled("DRAM Active ", Style::default().fg(MUTED)),
            Span::styled(format!("{:.0}%", dram_act * 100.0), Style::default().fg(util_color(dram_act)).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Memory Clock", Style::default().fg(MUTED)),
            Span::styled(format!(" {mem_clock:?} MHz"), Style::default().fg(CYAN)),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(ctrl_lines)
            .block(panel(" Controller & Clocks ", PINK)),
        chunks[1],
    );

    // Reliability & ECC
    let ecc_vol = sample.health.corrected_ecc_volatile.as_ref().map(|m| m.value).unwrap_or(0);
    let ecc_un = sample.health.uncorrected_ecc_volatile.as_ref().map(|m| m.value).unwrap_or(0);
    let retired_corr = sample.health.retired_pages_corrected.as_ref().map(|m| m.value).unwrap_or(0);
    let retired_uncorr = sample.health.retired_pages_uncorrected.as_ref().map(|m| m.value).unwrap_or(0);

    let ecc_lines = vec![
        Line::from(vec![
            Span::styled("ECC Volatile: ", Style::default().fg(MUTED)),
            Span::styled(format!("{ecc_vol} corr"), Style::default().fg(GREEN)),
            Span::raw(" · "),
            Span::styled(format!("{ecc_un} uncorr"), Style::default().fg(if ecc_un > 0 { RED } else { GREEN })),
        ]),
        Line::from(vec![
            Span::styled("Retired Pages:", Style::default().fg(MUTED)),
            Span::styled(format!(" {retired_corr} corr / {retired_uncorr} uncorr"), Style::default().fg(if retired_uncorr > 0 { RED } else { GREEN })),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(ecc_lines)
            .block(panel(" Memory Health & ECC ", GREEN)),
        chunks[2],
    );

    // Top VRAM Consumer Processes
    let mut proc_lines = Vec::new();
    if device.processes.is_empty() {
        proc_lines.push(Line::from(Span::styled("No active process VRAM allocations", Style::default().fg(MUTED))));
    } else {
        for proc in device.processes.iter().take(3) {
            proc_lines.push(Line::from(vec![
                Span::styled(format!("{:>6} ", proc.pid), Style::default().fg(GOLD)),
                Span::styled(short_name(proc.command.as_deref().unwrap_or("unknown"), 12), Style::default().fg(WHITE)),
                Span::raw(" "),
                Span::styled(fmt_bytes(proc.used_gpu_memory_bytes), Style::default().fg(AMBER).add_modifier(Modifier::BOLD)),
            ]));
        }
    }

    frame.render_widget(
        Paragraph::new(proc_lines)
            .block(panel(" Process VRAM Attribution ", CYAN)),
        chunks[3],
    );
}
