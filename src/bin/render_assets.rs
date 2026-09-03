// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::path::Path;

use chrono::Utc;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::{Color, Modifier};

use nv_toplike::cli::ViewMode;
use nv_toplike::model::*;
use nv_toplike::ui::{App, render};

#[derive(serde::Serialize)]
struct RenderedCell {
    symbol: String,
    fg: (u8, u8, u8),
    bg: (u8, u8, u8),
    bold: bool,
}

#[derive(serde::Serialize)]
struct FrameData {
    width: usize,
    height: usize,
    cells: Vec<Vec<RenderedCell>>,
}

fn color_to_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black => (13, 17, 23),
        Color::Red | Color::LightRed => (248, 81, 73),
        Color::Green | Color::LightGreen => (63, 185, 80),
        Color::Yellow | Color::LightYellow => (210, 153, 34),
        Color::Blue | Color::LightBlue => (88, 166, 255),
        Color::Magenta | Color::LightMagenta => (219, 97, 162),
        Color::Cyan | Color::LightCyan => (56, 189, 248),
        Color::Gray => (139, 148, 158),
        Color::DarkGray => (48, 54, 61),
        Color::White => (240, 246, 252),
        Color::Reset => (201, 209, 217),
        Color::Indexed(idx) => (idx, idx, idx),
    }
}

fn fixture_device(index: usize, name: &str, sm_count: u32, vram_gb: u64, util: f64, frame: u64) -> DeviceSnapshot {
    let now = Utc::now();
    let mut sample = AcceleratorSample::default();
    sample.utilization.gpu_ratio = Some(Metric::nvml(util, now, MetricScope::Device));
    sample.utilization.memory_controller_ratio = Some(Metric::nvml(0.34, now, MetricScope::Device));
    sample.utilization.tensor_active_ratio = Some(Metric::nvml(0.78, now, MetricScope::Device));
    sample.utilization.encoder_ratio = Some(Metric::nvml(0.0, now, MetricScope::Device));
    sample.utilization.decoder_ratio = Some(Metric::nvml(0.0, now, MetricScope::Device));

    let total = vram_gb * 1024 * 1024 * 1024;
    let used = (total as f64 * 0.25) as u64;
    sample.memory = MemorySample {
        total_bytes: Some(Metric::nvml(total, now, MetricScope::Device)),
        used_bytes: Some(Metric::nvml(used, now, MetricScope::Device)),
        free_bytes: Some(Metric::nvml(total - used, now, MetricScope::Device)),
        reserved_bytes: Some(Metric::nvml(0, now, MetricScope::Device)),
    };

    sample.clocks.sm_clock_mhz = Some(Metric::nvml(2520, now, MetricScope::Device));
    sample.clocks.memory_clock_mhz = Some(Metric::nvml(14000, now, MetricScope::Device));
    sample.clocks.performance_state = Some("P0".to_owned());

    sample.thermals.temperature_celsius = Some(Metric::nvml(54.0 + (frame as f64 * 0.1).sin() * 2.0, now, MetricScope::Device));
    sample.thermals.fan_percent = Some(Metric::nvml(45.0, now, MetricScope::Device));

    sample.power.power_watts = Some(Metric::nvml(142.0 + (frame as f64 * 0.15).cos() * 8.0, now, MetricScope::Device));
    sample.power.power_limit_watts = Some(Metric::nvml(300.0, now, MetricScope::Device));

    sample.links.pcie_tx_bytes_per_second = Some(Metric::nvml(2_576_980_377, now, MetricScope::Device));
    sample.links.pcie_rx_bytes_per_second = Some(Metric::nvml(8_698_753_024, now, MetricScope::Device));
    sample.links.pcie_generation = Some(5);
    sample.links.pcie_width = Some(16);

    sample.health.corrected_ecc_volatile = Some(Metric::nvml(0, now, MetricScope::Device));
    sample.health.uncorrected_ecc_volatile = Some(Metric::nvml(0, now, MetricScope::Device));
    sample.health.pcie_replay_counter = Some(Metric::nvml(0, now, MetricScope::Device));
    sample.health.retired_pages_corrected = Some(Metric::nvml(0, now, MetricScope::Device));
    sample.health.retired_pages_uncorrected = Some(Metric::nvml(0, now, MetricScope::Device));

    DeviceSnapshot {
        device: AcceleratorDevice {
            id: format!("GPU-{index}"),
            parent_id: None,
            display_index: Some(index as u32),
            pci_bus_id: Some(format!("0000:0{}:00.0", index + 1)),
            vendor: "NVIDIA".to_owned(),
            name: name.to_owned(),
            architecture: Some("Blackwell".to_owned()),
            entity_kind: EntityKind::PhysicalGpu,
            compute_units: Some(sm_count),
            memory_total_bytes: Some(total),
            mig_enabled: Some(false),
            capabilities: BTreeSet::new(),
        },
        sample,
        processes: vec![
            ProcessSample {
                pid: 412940,
                kind: ProcessKind::Compute,
                command: Some("python3".to_owned()),
                command_line: Some("vllm serve meta-llama/Llama-3-70B".to_owned()),
                used_gpu_memory_bytes: Some(24_051_816_857),
                sm_ratio: Some(0.48),
                memory_ratio: Some(0.24),
                encoder_ratio: None,
                decoder_ratio: None,
                gpu_instance_id: None,
                compute_instance_id: None,
            },
            ProcessSample {
                pid: 413812,
                kind: ProcessKind::Compute,
                command: Some("tritonserver".to_owned()),
                command_line: Some("tritonserver --model-repository=/models".to_owned()),
                used_gpu_memory_bytes: Some(1_288_490_188),
                sm_ratio: Some(0.04),
                memory_ratio: Some(0.01),
                encoder_ratio: None,
                decoder_ratio: None,
                gpu_instance_id: None,
                compute_instance_id: None,
            },
        ],
        stale: false,
        error: None,
    }
}

fn main() -> anyhow::Result<()> {
    let out_dir = Path::new("target/frames");
    fs::create_dir_all(out_dir)?;

    let width = 120;
    let height = 34;

    let modes = [
        ("overview", ViewMode::Overview),
        ("constellation", ViewMode::Constellation),
        ("memory", ViewMode::Memory),
        ("fabric", ViewMode::Fabric),
        ("fleet", ViewMode::Fleet),
    ];

    let total_frames = 30;

    for (name, mode) in modes {
        println!("Rendering {name} frames...");
        let mode_dir = out_dir.join(name);
        fs::create_dir_all(&mode_dir)?;

        let mut app = App::new(mode);
        let backend = TestBackend::new(width as u16, height as u16);
        let mut terminal = Terminal::new(backend)?;

        for f in 0..total_frames {
            app.frame = f as u64;
            let util = 0.52 + (f as f64 * 0.2).sin() * 0.12;
            let snapshot = Snapshot {
                captured_at: Utc::now(),
                driver_version: Some("610.43.02".to_owned()),
                nvml_version: Some("13.610.43".to_owned()),
                devices: vec![
                    fixture_device(0, "NVIDIA RTX PRO 6000 Blackwell", 188, 96, util, app.frame),
                    fixture_device(1, "NVIDIA RTX PRO 6000 Blackwell", 188, 96, util * 0.8, app.frame),
                ],
                topology: Vec::new(),
                ..Snapshot::empty(Utc::now())
            };
            app.observe(&snapshot);

            terminal.draw(|frame| render(frame, &app, Some(&snapshot), None))?;

            let buffer = terminal.backend().buffer();
            let mut rows = Vec::with_capacity(height);

            for y in 0..height {
                let mut row = Vec::with_capacity(width);
                for x in 0..width {
                    let cell = buffer.cell((x as u16, y as u16)).unwrap();
                    row.push(RenderedCell {
                        symbol: cell.symbol().to_owned(),
                        fg: color_to_rgb(cell.fg),
                        bg: color_to_rgb(cell.bg),
                        bold: cell.modifier.contains(Modifier::BOLD),
                    });
                }
                rows.push(row);
            }

            let frame_data = FrameData {
                width,
                height,
                cells: rows,
            };

            let frame_file = mode_dir.join(format!("frame_{:03}.json", f));
            let mut file = File::create(frame_file)?;
            serde_json::to_writer(&mut file, &frame_data)?;
        }
    }

    println!("All frame data exported to target/frames/");
    Ok(())
}
