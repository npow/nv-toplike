// SPDX-License-Identifier: Apache-2.0

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};

use crate::model::Snapshot;
use crate::ui::colors::*;
use crate::ui::{App, fmt_bytes, fmt_percent, fmt_temp, fmt_watts, panel, short_name};

pub fn render_fleet(frame: &mut Frame<'_>, area: Rect, app: &App, snapshot: &Snapshot) {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(area);

    // 1. Top Cluster Summary Deck
    render_cluster_summary(frame, main_layout[0], snapshot);

    // 2. Middle Fleet Grid Table
    render_fleet_table(frame, main_layout[1], app, snapshot);

    // 3. Bottom Per-GPU Visual Cards
    render_fleet_cards(frame, main_layout[2], app, snapshot);
}

fn render_cluster_summary(frame: &mut Frame<'_>, area: Rect, snapshot: &Snapshot) {
    let device_count = snapshot.devices.len();
    let total_vram_used: u64 = snapshot
        .devices
        .iter()
        .filter_map(|d| d.sample.memory.used_bytes.as_ref().map(|m| m.value))
        .sum();
    let total_vram_cap: u64 = snapshot
        .devices
        .iter()
        .filter_map(|d| d.sample.memory.total_bytes.as_ref().map(|m| m.value))
        .sum();

    let powers: Vec<f64> = snapshot
        .devices
        .iter()
        .filter_map(|d| d.sample.power.power_watts.as_ref().map(|m| m.value))
        .collect();
    let limits: Vec<f64> = snapshot
        .devices
        .iter()
        .filter_map(|d| d.sample.power.power_limit_watts.as_ref().map(|m| m.value))
        .collect();
    let temps: Vec<f64> = snapshot
        .devices
        .iter()
        .filter_map(|d| d.temperature_c())
        .collect();

    let power_str = if !powers.is_empty() {
        let tot_p: f64 = powers.iter().sum();
        let tot_l: f64 = limits.iter().sum();
        if tot_l > 0.0 {
            format!("{tot_p:.0}W / {tot_l:.0}W")
        } else {
            format!("{tot_p:.0}W")
        }
    } else {
        "N/A".to_owned()
    };

    let avg_temp = if !temps.is_empty() {
        Some(temps.iter().sum::<f64>() / temps.len() as f64)
    } else {
        None
    };

    let summary_line = Line::from(vec![
        Span::styled(
            " Fleet Cluster ",
            Style::default()
                .fg(Color::Black)
                .bg(CYAN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!(
                "{device_count} Device{}",
                if device_count == 1 { "" } else { "s" }
            ),
            Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" · Cluster VRAM: "),
        Span::styled(
            format!(
                "{} / {}",
                fmt_bytes(Some(total_vram_used)),
                fmt_bytes(Some(total_vram_cap))
            ),
            Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" · Cluster Power: "),
        Span::styled(
            power_str,
            Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" · Avg Temp: "),
        Span::styled(
            fmt_temp(avg_temp),
            Style::default()
                .fg(heat_color(avg_temp.unwrap_or(35.0)))
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    frame.render_widget(
        Paragraph::new(summary_line).block(panel(" Fleet Overview ", CYAN)),
        area,
    );
}

fn render_fleet_table(frame: &mut Frame<'_>, area: Rect, app: &App, snapshot: &Snapshot) {
    let rows = snapshot.devices.iter().enumerate().map(|(index, device)| {
        let health_fault = device.sample.health.observations.iter().any(|value| {
            value.contains("uncorrected") || value.contains("slowdown") || value.contains("brake")
        });
        let sm_str = device
            .device
            .compute_units
            .map_or_else(|| "-".to_owned(), |c| c.to_string());

        Row::new(vec![
            Cell::from(if index == app.selected { "▶" } else { " " }),
            Cell::from(
                device
                    .device
                    .display_index
                    .map_or_else(|| "-".to_owned(), |v| v.to_string()),
            ),
            Cell::from(short_name(&device.device.name, 28)),
            Cell::from(device.device.architecture.as_deref().unwrap_or("N/A")),
            Cell::from(sm_str),
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
            Cell::from(if health_fault { "WARN" } else { "NOMINAL" }),
        ])
        .style(if health_fault {
            Style::default().fg(RED)
        } else if index == app.selected {
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(WHITE)
        })
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(4),
            Constraint::Min(16),
            Constraint::Length(10),
            Constraint::Length(5),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(19),
            Constraint::Length(9),
            Constraint::Length(7),
            Constraint::Length(8),
        ],
    )
    .header(
        Row::new([
            "", "GPU", "MODEL", "ARCH", "SMs", "GPU", "MEM", "VRAM", "POWER", "TEMP", "HEALTH",
        ])
        .style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
    )
    .column_spacing(1)
    .block(panel(
        " Devices · UUID-stable NVML enumeration (←/→ selects GPU) ",
        CYAN,
    ));

    frame.render_widget(table, area);
}

fn render_fleet_cards(frame: &mut Frame<'_>, area: Rect, app: &App, snapshot: &Snapshot) {
    let count = snapshot.devices.len();
    if count == 0 {
        return;
    }

    let cols = count.min(4);
    let card_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Percentage(100 / cols as u16); cols])
        .split(area);

    for (index, device) in snapshot.devices.iter().take(cols).enumerate() {
        let is_sel = index == app.selected;
        let gpu_ratio = device.gpu_ratio().unwrap_or(0.0);
        let mem_fill = device.memory_fill_ratio().unwrap_or(0.0);
        let temp = device.temperature_c();
        let pwr = device.sample.power.power_watts.as_ref().map(|m| m.value);

        let mut lines = Vec::new();

        lines.push(Line::from(vec![
            Span::styled(
                format!(
                    "GPU {}: ",
                    device.device.display_index.unwrap_or(index as u32)
                ),
                Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                short_name(&device.device.name, 16),
                Style::default().fg(WHITE),
            ),
        ]));

        // Mini SM bar
        let sm_bar_width = 10;
        let filled_sm = (gpu_ratio * sm_bar_width as f64).round() as usize;
        let sm_bar: String = (0..sm_bar_width)
            .map(|i| if i < filled_sm { '▓' } else { '░' })
            .collect();
        lines.push(Line::from(vec![
            Span::styled("GPU  ", Style::default().fg(MUTED)),
            Span::styled(sm_bar, Style::default().fg(util_color(gpu_ratio))),
            Span::styled(
                format!(" {:>3.0}%", gpu_ratio * 100.0),
                Style::default().fg(CYAN),
            ),
        ]));

        // Mini VRAM bar
        let filled_mem = (mem_fill * sm_bar_width as f64).round() as usize;
        let mem_bar: String = (0..sm_bar_width)
            .map(|i| if i < filled_mem { '▓' } else { '░' })
            .collect();
        lines.push(Line::from(vec![
            Span::styled("VRAM ", Style::default().fg(MUTED)),
            Span::styled(mem_bar, Style::default().fg(AMBER)),
            Span::styled(
                format!(" {:>3.0}%", mem_fill * 100.0),
                Style::default().fg(AMBER),
            ),
        ]));

        // Temp & Power
        lines.push(Line::from(vec![
            Span::styled(
                fmt_temp(temp),
                Style::default()
                    .fg(heat_color(temp.unwrap_or(35.0)))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" · "),
            Span::styled(fmt_watts(pwr), Style::default().fg(GREEN)),
        ]));

        let border_color = if is_sel { CYAN } else { DARK_GRAY };
        frame.render_widget(
            Paragraph::new(lines).block(panel(&format!(" Device {} Card ", index), border_color)),
            card_chunks[index],
        );
    }
}
