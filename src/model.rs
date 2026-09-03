// SPDX-License-Identifier: Apache-2.0

//! Vendor-aware, renderer-neutral telemetry models.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricSource {
    Nvml,
    Dcgm,
    Cupti,
    Prometheus,
    Derived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricScope {
    Device,
    MigInstance,
    Process,
    Link,
    Workload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricQuality {
    Direct,
    Derived,
    Estimated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metric<T> {
    pub value: T,
    pub source: MetricSource,
    pub scope: MetricScope,
    pub sampled_at: DateTime<Utc>,
    pub quality: MetricQuality,
}

impl<T> Metric<T> {
    pub fn nvml(value: T, sampled_at: DateTime<Utc>, scope: MetricScope) -> Self {
        Self {
            value,
            source: MetricSource::Nvml,
            scope,
            sampled_at,
            quality: MetricQuality::Direct,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    PhysicalGpu,
    MigDevice,
}

impl EntityKind {
    #[must_use]
    pub const fn metric_scope(self) -> MetricScope {
        match self {
            Self::PhysicalGpu => MetricScope::Device,
            Self::MigDevice => MetricScope::MigInstance,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceleratorDevice {
    /// Stable NVML UUID. This is the primary identity, never the display index.
    pub id: String,
    pub parent_id: Option<String>,
    pub display_index: Option<u32>,
    pub pci_bus_id: Option<String>,
    pub vendor: String,
    pub name: String,
    pub architecture: Option<String>,
    pub entity_kind: EntityKind,
    /// Streaming multiprocessor count, including a MIG allocation when exposed.
    pub compute_units: Option<u32>,
    pub memory_total_bytes: Option<u64>,
    pub mig_enabled: Option<bool>,
    pub capabilities: BTreeSet<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ThermalSample {
    pub temperature_celsius: Option<Metric<f64>>,
    pub slowdown_threshold_celsius: Option<Metric<f64>>,
    pub shutdown_threshold_celsius: Option<Metric<f64>>,
    pub fan_percent: Option<Metric<f64>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PowerSample {
    pub power_watts: Option<Metric<f64>>,
    pub power_limit_watts: Option<Metric<f64>>,
    pub energy_millijoules: Option<Metric<u64>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClockSample {
    pub graphics_clock_mhz: Option<Metric<u32>>,
    pub sm_clock_mhz: Option<Metric<u32>>,
    pub memory_clock_mhz: Option<Metric<u32>>,
    pub video_clock_mhz: Option<Metric<u32>>,
    pub performance_state: Option<String>,
    pub throttle_reasons: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UtilizationSample {
    /// Fraction of the NVML sampling period with one or more kernels executing.
    pub gpu_ratio: Option<Metric<f64>>,
    /// Fraction of the NVML sampling period with global-memory reads or writes.
    pub memory_controller_ratio: Option<Metric<f64>>,
    pub encoder_ratio: Option<Metric<f64>>,
    pub decoder_ratio: Option<Metric<f64>>,
    /// Reserved for DCGM enrichment; never synthesized from `gpu_ratio`.
    pub sm_active_ratio: Option<Metric<f64>>,
    pub sm_occupancy_ratio: Option<Metric<f64>>,
    pub tensor_active_ratio: Option<Metric<f64>>,
    pub dram_active_ratio: Option<Metric<f64>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MemorySample {
    pub used_bytes: Option<Metric<u64>>,
    pub total_bytes: Option<Metric<u64>>,
    pub free_bytes: Option<Metric<u64>>,
    pub reserved_bytes: Option<Metric<u64>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LinkSample {
    /// GPU-originated traffic as reported by NVML's `PCIe` TX counter.
    pub pcie_tx_bytes_per_second: Option<Metric<u64>>,
    /// GPU-destined traffic as reported by NVML's `PCIe` RX counter.
    pub pcie_rx_bytes_per_second: Option<Metric<u64>>,
    pub pcie_generation: Option<u32>,
    pub pcie_width: Option<u32>,
    pub nvlink_tx_bytes_per_second: Option<Metric<u64>>,
    pub nvlink_rx_bytes_per_second: Option<Metric<u64>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HealthSample {
    pub corrected_ecc_volatile: Option<Metric<u64>>,
    pub uncorrected_ecc_volatile: Option<Metric<u64>>,
    pub retired_pages_corrected: Option<Metric<u64>>,
    pub retired_pages_uncorrected: Option<Metric<u64>>,
    pub pcie_replay_counter: Option<Metric<u64>>,
    /// Health observations reported by accessible device counters.
    pub observations: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AcceleratorSample {
    pub thermals: ThermalSample,
    pub power: PowerSample,
    pub clocks: ClockSample,
    pub utilization: UtilizationSample,
    pub memory: MemorySample,
    pub links: LinkSample,
    pub health: HealthSample,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessKind {
    Compute,
    Graphics,
    ComputeAndGraphics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessSample {
    pub pid: u32,
    pub kind: ProcessKind,
    pub command: Option<String>,
    pub command_line: Option<String>,
    pub used_gpu_memory_bytes: Option<u64>,
    pub sm_ratio: Option<f64>,
    pub memory_ratio: Option<f64>,
    pub encoder_ratio: Option<f64>,
    pub decoder_ratio: Option<f64>,
    pub gpu_instance_id: Option<u32>,
    pub compute_instance_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopologyKind {
    MigParent,
    PciInternal,
    PciSingleSwitch,
    PciMultiSwitch,
    PciHostBridge,
    NumaNode,
    System,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyEdge {
    pub from: String,
    pub to: String,
    pub kind: TopologyKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceSnapshot {
    pub device: AcceleratorDevice,
    pub sample: AcceleratorSample,
    pub processes: Vec<ProcessSample>,
    pub stale: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub backend: String,
    pub driver_version: Option<String>,
    pub nvml_version: Option<String>,
    pub captured_at: DateTime<Utc>,
    pub devices: Vec<DeviceSnapshot>,
    pub topology: Vec<TopologyEdge>,
    pub warnings: Vec<String>,
}

impl Snapshot {
    #[must_use]
    pub fn empty(now: DateTime<Utc>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            backend: "nvml".to_owned(),
            driver_version: None,
            nvml_version: None,
            captured_at: now,
            devices: Vec::new(),
            topology: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

impl DeviceSnapshot {
    #[must_use]
    pub fn gpu_ratio(&self) -> Option<f64> {
        self.sample.utilization.gpu_ratio.as_ref().map(|m| m.value)
    }

    #[must_use]
    pub fn memory_activity_ratio(&self) -> Option<f64> {
        self.sample
            .utilization
            .dram_active_ratio
            .as_ref()
            .or(self.sample.utilization.memory_controller_ratio.as_ref())
            .map(|m| m.value)
    }

    #[must_use]
    pub fn memory_fill_ratio(&self) -> Option<f64> {
        let used = self.sample.memory.used_bytes.as_ref()?.value as f64;
        let total = self.sample.memory.total_bytes.as_ref()?.value as f64;
        (total > 0.0).then_some((used / total).clamp(0.0, 1.0))
    }

    #[must_use]
    pub fn power_ratio(&self) -> Option<f64> {
        let power = self.sample.power.power_watts.as_ref()?.value;
        let limit = self.sample.power.power_limit_watts.as_ref()?.value;
        (limit > 0.0).then_some((power / limit).clamp(0.0, 1.5))
    }

    #[must_use]
    pub fn temperature_c(&self) -> Option<f64> {
        self.sample
            .thermals
            .temperature_celsius
            .as_ref()
            .map(|m| m.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_memory_total_has_no_fill_ratio() {
        let now = Utc::now();
        let mut snapshot = test_device();
        snapshot.sample.memory.used_bytes = Some(Metric::nvml(12, now, MetricScope::Device));
        snapshot.sample.memory.total_bytes = Some(Metric::nvml(0, now, MetricScope::Device));
        assert_eq!(snapshot.memory_fill_ratio(), None);
    }

    #[test]
    fn ratios_are_clamped_for_rendering() {
        let now = Utc::now();
        let mut snapshot = test_device();
        snapshot.sample.memory.used_bytes = Some(Metric::nvml(120, now, MetricScope::Device));
        snapshot.sample.memory.total_bytes = Some(Metric::nvml(100, now, MetricScope::Device));
        assert_eq!(snapshot.memory_fill_ratio(), Some(1.0));
    }

    #[test]
    fn normalized_snapshot_round_trips() {
        let snapshot = Snapshot {
            devices: vec![test_device()],
            ..Snapshot::empty(Utc::now())
        };
        let json = serde_json::to_string(&snapshot).expect("serialize");
        let decoded: Snapshot = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.schema_version, SCHEMA_VERSION);
        assert_eq!(decoded.devices[0].device.id, "GPU-test");
    }

    fn test_device() -> DeviceSnapshot {
        DeviceSnapshot {
            device: AcceleratorDevice {
                id: "GPU-test".to_owned(),
                parent_id: None,
                display_index: Some(0),
                pci_bus_id: Some("0000:01:00.0".to_owned()),
                vendor: "NVIDIA".to_owned(),
                name: "Test GPU".to_owned(),
                architecture: Some("Test".to_owned()),
                entity_kind: EntityKind::PhysicalGpu,
                compute_units: Some(8),
                memory_total_bytes: Some(100),
                mig_enabled: Some(false),
                capabilities: BTreeSet::new(),
            },
            sample: AcceleratorSample::default(),
            processes: Vec::new(),
            stale: false,
            error: None,
        }
    }
}
