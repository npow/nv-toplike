// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::model::{Snapshot, TopologyKind};
use crate::ui::colors::*;
use crate::ui::{fmt_rate, panel, short_name, short_uuid};

pub fn render_fabric(frame: &mut Frame<'_>, area: Rect, snapshot: &Snapshot) {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(4),
        ])
        .split(area);

    let header_line = Line::from(vec![
        Span::styled(
            " Fabric Map ",
            Style::default().fg(Color::Black).bg(CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  Direct NVML Hardware Interconnect & Bus Topology · "),
        Span::styled(
            format!("{} GPU Device{}", snapshot.devices.len(), if snapshot.devices.len() == 1 { "" } else { "s" }),
            Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" · Inter-GPU Links: "),
        Span::styled(
            format!("{}", snapshot.topology.len()),
            Style::default().fg(if snapshot.topology.is_empty() { MUTED } else { GREEN }),
        ),
    ]);

    frame.render_widget(
        Paragraph::new(header_line)
            .block(panel(" Interconnect Fabric ", CYAN)),
        main_layout[0],
    );

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

    render_fabric_diagram(frame, body_chunks[0], snapshot);

    if body_chunks[1].width > 15 {
        render_fabric_link_deck(frame, body_chunks[1], snapshot);
    }

    // Bottom Semantics
    frame.render_widget(
        Paragraph::new(
            "PCIe throughput rates are measured per-device NVML counters. Inter-GPU edges reflect direct NVML topology queries (Single Switch, Multi Switch, Host Bridge, NUMA Node, NVLink).",
        )
        .style(Style::default().fg(MUTED))
        .wrap(Wrap { trim: true })
        .block(panel(" Interconnect Semantics ", MUTED)),
        main_layout[2],
    );
}

fn render_fabric_diagram(frame: &mut Frame<'_>, area: Rect, snapshot: &Snapshot) {
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

    let mut lines = Vec::new();

    if snapshot.topology.is_empty() {
        // Render rich Host-to-Device single GPU bus hierarchy diagram
        if let Some(device) = snapshot.devices.first() {
            let gen_str = device
                .sample
                .links
                .pcie_generation
                .map_or_else(|| "?".to_owned(), |v| v.to_string());
            let width_str = device
                .sample
                .links
                .pcie_width
                .map_or_else(|| "?".to_owned(), |v| v.to_string());
            let tx = fmt_rate(
                device
                    .sample
                    .links
                    .pcie_tx_bytes_per_second
                    .as_ref()
                    .map(|m| m.value),
            );
            let rx = fmt_rate(
                device
                    .sample
                    .links
                    .pcie_rx_bytes_per_second
                    .as_ref()
                    .map(|m| m.value),
            );

            lines.push(Line::from(vec![Span::styled(
                "┌─ [ HOST CPU ROOT COMPLEX / SYSTEM MEMORY ]",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::from(vec![Span::styled(
                "│   │",
                Style::default().fg(MUTED),
            )]));
            lines.push(Line::from(vec![
                Span::styled(
                    "│   ▼ PCIe Bus Bridge (Negotiated: ",
                    Style::default().fg(CYAN),
                ),
                Span::styled(
                    format!("Gen{gen_str} x{width_str}"),
                    Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" · Direct DMA)", Style::default().fg(CYAN)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("│   │   Traffic: TX ", Style::default().fg(MUTED)),
                Span::styled(format!("{tx:>10}"), Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
                Span::styled(" · RX ", Style::default().fg(MUTED)),
                Span::styled(format!("{rx:>10}"), Style::default().fg(PINK).add_modifier(Modifier::BOLD)),
            ]));
            lines.push(Line::from(vec![Span::styled(
                "│   │",
                Style::default().fg(MUTED),
            )]));
            lines.push(Line::from(vec![
                Span::styled("└───▼─► [ ", Style::default().fg(MUTED)),
                Span::styled(
                    &device.device.name,
                    Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ]", Style::default().fg(MUTED)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("        PCI Bus: ", Style::default().fg(MUTED)),
                Span::styled(
                    device.device.pci_bus_id.as_deref().unwrap_or("N/A"),
                    Style::default().fg(WHITE),
                ),
                Span::styled(" · UUID: ", Style::default().fg(MUTED)),
                Span::styled(short_uuid(&device.device.id), Style::default().fg(CYAN)),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Single GPU system: displaying host-to-device PCIe interconnect architecture.",
                Style::default().fg(MUTED),
            )));
        }
    } else {
        // Multi-GPU list and topology graph
        for device in &snapshot.devices {
            lines.push(Line::from(vec![
                Span::styled(
                    "● ",
                    Style::default().fg(heat_color(device.temperature_c().unwrap_or(35.0))),
                ),
                Span::styled(
                    name_by_id
                        .get(device.device.id.as_str())
                        .cloned()
                        .unwrap_or_else(|| short_uuid(&device.device.id)),
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(
                    "  {}  PCIe Gen{} x{}  TX {:>10}  RX {:>10}",
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
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Interconnect Relationships:",
            Style::default().fg(MUTED),
        )));

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
            .block(panel(" Interconnect Topology Graph ", CYAN)),
        area,
    );
}

fn render_fabric_link_deck(frame: &mut Frame<'_>, area: Rect, snapshot: &Snapshot) {
    let mut lines = Vec::new();

    for (index, device) in snapshot.devices.iter().enumerate() {
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
        let replay = sample
            .health
            .pcie_replay_counter
            .as_ref()
            .map(|m| m.value)
            .unwrap_or(0);

        lines.push(Line::from(vec![
            Span::styled(
                format!("GPU {index}: "),
                Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                short_name(&device.device.name, 18),
                Style::default().fg(WHITE),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Link: ", Style::default().fg(MUTED)),
            Span::styled(
                format!(
                    "PCIe Gen{} x{}",
                    sample
                        .links
                        .pcie_generation
                        .map_or_else(|| "?".to_owned(), |v| v.to_string()),
                    sample
                        .links
                        .pcie_width
                        .map_or_else(|| "?".to_owned(), |v| v.to_string())
                ),
                Style::default().fg(GREEN),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Traffic: ", Style::default().fg(MUTED)),
            Span::styled(
                format!("TX {:>10}", fmt_rate(pcie_tx)),
                Style::default().fg(CYAN),
            ),
            Span::raw(" · "),
            Span::styled(
                format!("RX {:>10}", fmt_rate(pcie_rx)),
                Style::default().fg(PINK),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Replays: ", Style::default().fg(MUTED)),
            Span::styled(
                format!("{replay}"),
                Style::default().fg(if replay > 0 { RED } else { GREEN }),
            ),
        ]));
        lines.push(Line::from(""));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(panel(" Link Health & Counters ", GREEN)),
        area,
    );
}

fn topology_color(kind: &TopologyKind) -> Color {
    match kind {
        TopologyKind::MigParent | TopologyKind::PciInternal => GREEN,
        TopologyKind::PciSingleSwitch => CYAN,
        TopologyKind::PciMultiSwitch | TopologyKind::PciHostBridge => AMBER,
        TopologyKind::NumaNode | TopologyKind::System | TopologyKind::Unknown => MUTED,
    }
}
