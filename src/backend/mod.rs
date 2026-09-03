// SPDX-License-Identifier: Apache-2.0

pub mod cuda;
pub mod nvml;

use crate::model::Snapshot;

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("NVML initialization failed: {0}")]
    Initialization(String),
    #[error("device discovery failed: {0}")]
    Discovery(String),
    #[error("no NVIDIA GPU matched the requested selection")]
    NoDevices,
    #[error("telemetry collection failed: {0}")]
    Collection(String),
}

pub trait TelemetryBackend: Send {
    fn sample(&mut self) -> Result<Snapshot, BackendError>;
}
