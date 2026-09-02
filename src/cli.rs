// SPDX-License-Identifier: Apache-2.0

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BackendChoice {
    Nvml,
    Dcgm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ViewMode {
    Overview,
    Constellation,
    Memory,
    Fabric,
    Fleet,
}

impl ViewMode {
    pub const ALL: [Self; 5] = [
        Self::Overview,
        Self::Constellation,
        Self::Memory,
        Self::Fabric,
        Self::Fleet,
    ];

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Constellation => "SM Constellation",
            Self::Memory => "Memory Foundry",
            Self::Fabric => "Fabric Map",
            Self::Fleet => "Fleet",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum MigView {
    Physical,
    Instances,
    All,
}

/// Hardware-grounded NVIDIA GPU telemetry in the terminal.
#[derive(Debug, Clone, Parser)]
#[command(version, about)]
pub struct Cli {
    /// Telemetry backend. DCGM is reserved for the enrichment phase.
    #[arg(long, value_enum, default_value_t = BackendChoice::Nvml)]
    pub backend: BackendChoice,

    /// Initial visualization.
    #[arg(long, value_enum, default_value_t = ViewMode::Overview)]
    pub mode: ViewMode,

    /// Select a physical display index or an exact GPU/MIG UUID.
    #[arg(long)]
    pub device: Option<String>,

    /// Choose physical GPUs, MIG devices, or both.
    #[arg(long, value_enum, default_value_t = MigView::Physical)]
    pub mig: MigView,

    /// UI render rate.
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u16).range(1..=60))]
    pub fps: u16,

    /// NVML collection interval in milliseconds.
    #[arg(long, default_value_t = 200, value_parser = clap::value_parser!(u64).range(100..=60_000))]
    pub sample_ms: u64,

    /// Emit one normalized telemetry snapshot as JSON and exit.
    #[arg(long)]
    pub json: bool,

    /// Pretty-print JSON output.
    #[arg(long, requires = "json")]
    pub pretty: bool,
}
