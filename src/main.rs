// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::{Context, bail};
use clap::Parser;

use nv_toplike::backend::TelemetryBackend;
use nv_toplike::backend::nvml::{NvmlBackend, NvmlConfig};
use nv_toplike::cli::{BackendChoice, Cli};
use nv_toplike::collector::Collector;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if cli.backend == BackendChoice::Dcgm {
        bail!("DCGM backend is not supported in this build; use --backend nvml");
    }

    let backend_config = NvmlConfig {
        selector: cli.device.clone(),
        mig_view: cli.mig,
    };
    if cli.json {
        let mut backend = NvmlBackend::new(backend_config)?;
        let snapshot = backend.sample()?;
        let output = if cli.pretty {
            serde_json::to_string_pretty(&snapshot)
        } else {
            serde_json::to_string(&snapshot)
        }
        .context("failed to serialize normalized telemetry")?;
        println!("{output}");
        return Ok(());
    }

    let collector = Collector::spawn(backend_config, Duration::from_millis(cli.sample_ms))?;
    nv_toplike::ui::run(&collector, cli.mode, cli.fps)
}
