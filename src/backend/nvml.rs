// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use chrono::Utc;
use nvml_wrapper::Nvml;
use nvml_wrapper::bitmasks::device::ThrottleReasons;
#[cfg(target_os = "linux")]
use nvml_wrapper::enum_wrappers::device::TopologyLevel;
use nvml_wrapper::enum_wrappers::device::{
    Clock, EccCounter, MemoryError, PcieUtilCounter, PerformanceState, RetirementCause,
    TemperatureSensor, TemperatureThreshold,
};
use nvml_wrapper::enums::device::UsedGpuMemory;

use crate::backend::{BackendError, TelemetryBackend};
use crate::cli::MigView;
use crate::model::{
    AcceleratorDevice, AcceleratorSample, ClockSample, DeviceSnapshot, EntityKind, HealthSample,
    LinkSample, MemorySample, Metric, PowerSample, ProcessKind, ProcessSample, SCHEMA_VERSION,
    Snapshot, ThermalSample, TopologyEdge, TopologyKind, UtilizationSample,
};

#[derive(Debug, Clone)]
pub struct NvmlConfig {
    pub selector: Option<String>,
    pub mig_view: MigView,
}

impl Default for NvmlConfig {
    fn default() -> Self {
        Self {
            selector: None,
            mig_view: MigView::Physical,
        }
    }
}

#[derive(Debug, Clone)]
struct EntitySpec {
    physical_index: u32,
    mig_index: Option<u32>,
    device: AcceleratorDevice,
}

pub struct NvmlBackend {
    nvml: Nvml,
    entities: Vec<EntitySpec>,
    topology: Vec<TopologyEdge>,
    driver_version: Option<String>,
    nvml_version: Option<String>,
}

impl NvmlBackend {
    pub fn new(config: NvmlConfig) -> Result<Self, BackendError> {
        let nvml = Nvml::init().map_err(|error| BackendError::Initialization(error.to_string()))?;
        let driver_version = nvml.sys_driver_version().ok();
        let nvml_version = nvml.sys_nvml_version().ok();
        let device_count = nvml
            .device_count()
            .map_err(|error| BackendError::Discovery(error.to_string()))?;

        let cuda_table = crate::backend::cuda::CudaDeviceTable::query();
        let mut entities = Vec::new();
        for physical_index in 0..device_count {
            let physical = nvml
                .device_by_index(physical_index)
                .map_err(|error| BackendError::Discovery(error.to_string()))?;
            let physical_uuid = physical
                .uuid()
                .map_err(|error| BackendError::Discovery(error.to_string()))?;
            let mig_enabled = physical.mig_mode().ok().map(|mode| mode.current != 0);

            if matches!(config.mig_view, MigView::Physical | MigView::All) {
                entities.push(EntitySpec {
                    physical_index,
                    mig_index: None,
                    device: discover_device(
                        &physical,
                        physical_index,
                        EntityKind::PhysicalGpu,
                        None,
                        mig_enabled,
                        &cuda_table,
                    ),
                });
            }

            if matches!(config.mig_view, MigView::Instances | MigView::All)
                && mig_enabled == Some(true)
                && let Ok(max_mig_devices) = physical.mig_device_count()
            {
                for mig_index in 0..max_mig_devices {
                    let Ok(mig) = physical.mig_device_by_index(mig_index) else {
                        continue;
                    };
                    entities.push(EntitySpec {
                        physical_index,
                        mig_index: Some(mig_index),
                        device: discover_device(
                            &mig,
                            physical_index,
                            EntityKind::MigDevice,
                            Some(physical_uuid.clone()),
                            Some(true),
                            &cuda_table,
                        ),
                    });
                }
            }
        }

        if let Some(selector) = config.selector.as_deref() {
            entities.retain(|entity| selector_matches(entity, selector));
        }
        if entities.is_empty() {
            return Err(BackendError::NoDevices);
        }

        let topology = discover_topology(&nvml, &entities);
        Ok(Self {
            nvml,
            entities,
            topology,
            driver_version,
            nvml_version,
        })
    }

    fn sample_entity(&self, spec: &EntitySpec) -> DeviceSnapshot {
        let now = Utc::now();
        let scope = spec.device.entity_kind.metric_scope();
        let handle = self
            .nvml
            .device_by_index(spec.physical_index)
            .and_then(|physical| match spec.mig_index {
                Some(index) => physical.mig_device_by_index(index),
                None => Ok(physical),
            });
        let device = match handle {
            Ok(device) => device,
            Err(error) => {
                return DeviceSnapshot {
                    device: spec.device.clone(),
                    sample: AcceleratorSample::default(),
                    processes: Vec::new(),
                    stale: true,
                    error: Some(error.to_string()),
                };
            }
        };

        let utilization = device.utilization_rates().ok();
        let memory = device.memory_info().ok();
        let power_watts = device.power_usage().ok().map(|mw| f64::from(mw) / 1_000.0);
        let power_limit_watts = device
            .enforced_power_limit()
            .ok()
            .map(|mw| f64::from(mw) / 1_000.0);
        let temperature = device
            .temperature(TemperatureSensor::Gpu)
            .ok()
            .map(f64::from);
        let slowdown_temperature = device
            .temperature_threshold(TemperatureThreshold::Slowdown)
            .ok()
            .map(f64::from);
        let shutdown_temperature = device
            .temperature_threshold(TemperatureThreshold::Shutdown)
            .ok()
            .map(f64::from);
        let fan_percent = device.fan_speed(0).ok().map(f64::from);
        let throttle_reasons = device.current_throttle_reasons().ok();

        let mut sample = AcceleratorSample {
            thermals: ThermalSample {
                temperature_celsius: temperature.map(|value| Metric::nvml(value, now, scope)),
                slowdown_threshold_celsius: slowdown_temperature
                    .map(|value| Metric::nvml(value, now, scope)),
                shutdown_threshold_celsius: shutdown_temperature
                    .map(|value| Metric::nvml(value, now, scope)),
                fan_percent: fan_percent.map(|value| Metric::nvml(value, now, scope)),
            },
            power: PowerSample {
                power_watts: power_watts.map(|value| Metric::nvml(value, now, scope)),
                power_limit_watts: power_limit_watts.map(|value| Metric::nvml(value, now, scope)),
                energy_millijoules: device
                    .total_energy_consumption()
                    .ok()
                    .map(|value| Metric::nvml(value, now, scope)),
            },
            clocks: ClockSample {
                graphics_clock_mhz: device
                    .clock_info(Clock::Graphics)
                    .ok()
                    .map(|value| Metric::nvml(value, now, scope)),
                sm_clock_mhz: device
                    .clock_info(Clock::SM)
                    .ok()
                    .map(|value| Metric::nvml(value, now, scope)),
                memory_clock_mhz: device
                    .clock_info(Clock::Memory)
                    .ok()
                    .map(|value| Metric::nvml(value, now, scope)),
                video_clock_mhz: device
                    .clock_info(Clock::Video)
                    .ok()
                    .map(|value| Metric::nvml(value, now, scope)),
                performance_state: device.performance_state().ok().map(pstate_name),
                throttle_reasons: throttle_reasons
                    .map(throttle_reason_names)
                    .unwrap_or_default(),
            },
            utilization: UtilizationSample {
                gpu_ratio: utilization
                    .as_ref()
                    .map(|value| Metric::nvml(percent_ratio(value.gpu), now, scope)),
                memory_controller_ratio: utilization
                    .as_ref()
                    .map(|value| Metric::nvml(percent_ratio(value.memory), now, scope)),
                encoder_ratio: device
                    .encoder_utilization()
                    .ok()
                    .map(|value| Metric::nvml(percent_ratio(value.utilization), now, scope)),
                decoder_ratio: device
                    .decoder_utilization()
                    .ok()
                    .map(|value| Metric::nvml(percent_ratio(value.utilization), now, scope)),
                sm_active_ratio: None,
                sm_occupancy_ratio: None,
                tensor_active_ratio: None,
                dram_active_ratio: None,
            },
            memory: MemorySample {
                used_bytes: memory
                    .as_ref()
                    .map(|value| Metric::nvml(value.used, now, scope)),
                total_bytes: memory
                    .as_ref()
                    .map(|value| Metric::nvml(value.total, now, scope)),
                free_bytes: memory
                    .as_ref()
                    .map(|value| Metric::nvml(value.free, now, scope)),
                reserved_bytes: memory
                    .as_ref()
                    .map(|value| Metric::nvml(value.reserved, now, scope)),
            },
            links: LinkSample {
                // NVML reports KiB/s over an internal 20 ms measurement interval.
                pcie_tx_bytes_per_second: device
                    .pcie_throughput(PcieUtilCounter::Send)
                    .ok()
                    .map(|value| Metric::nvml(u64::from(value) * 1_024, now, scope)),
                pcie_rx_bytes_per_second: device
                    .pcie_throughput(PcieUtilCounter::Receive)
                    .ok()
                    .map(|value| Metric::nvml(u64::from(value) * 1_024, now, scope)),
                pcie_generation: device.current_pcie_link_gen().ok(),
                pcie_width: device.current_pcie_link_width().ok(),
                nvlink_tx_bytes_per_second: None,
                nvlink_rx_bytes_per_second: None,
            },
            health: HealthSample {
                corrected_ecc_volatile: device
                    .total_ecc_errors(MemoryError::Corrected, EccCounter::Volatile)
                    .ok()
                    .map(|value| Metric::nvml(value, now, scope)),
                uncorrected_ecc_volatile: device
                    .total_ecc_errors(MemoryError::Uncorrected, EccCounter::Volatile)
                    .ok()
                    .map(|value| Metric::nvml(value, now, scope)),
                retired_pages_corrected: device
                    .retired_pages(RetirementCause::MultipleSingleBitEccErrors)
                    .ok()
                    .map(|pages| Metric::nvml(pages.len() as u64, now, scope)),
                retired_pages_uncorrected: device
                    .retired_pages(RetirementCause::DoubleBitEccError)
                    .ok()
                    .map(|pages| Metric::nvml(pages.len() as u64, now, scope)),
                pcie_replay_counter: device
                    .pcie_replay_counter()
                    .ok()
                    .map(|value| Metric::nvml(u64::from(value), now, scope)),
                observations: Vec::new(),
            },
        };
        populate_health_observations(&mut sample, throttle_reasons);

        let mut accelerator = spec.device.clone();
        accelerator.capabilities = capabilities_for(&sample);

        DeviceSnapshot {
            device: accelerator,
            sample,
            processes: collect_processes(&device),
            stale: false,
            error: None,
        }
    }
}

impl TelemetryBackend for NvmlBackend {
    fn sample(&mut self) -> Result<Snapshot, BackendError> {
        let captured_at = Utc::now();
        let devices = self
            .entities
            .iter()
            .map(|entity| self.sample_entity(entity))
            .collect::<Vec<_>>();

        if devices.iter().all(|device| device.error.is_some()) {
            return Err(BackendError::Collection(
                "all visible devices failed during this sample".to_owned(),
            ));
        }

        Ok(Snapshot {
            schema_version: SCHEMA_VERSION,
            backend: "nvml".to_owned(),
            driver_version: self.driver_version.clone(),
            nvml_version: self.nvml_version.clone(),
            captured_at,
            devices,
            topology: self.topology.clone(),
            warnings: Vec::new(),
        })
    }
}

fn discover_device(
    device: &nvml_wrapper::Device<'_>,
    physical_index: u32,
    entity_kind: EntityKind,
    parent_id: Option<String>,
    mig_enabled: Option<bool>,
    cuda_table: &crate::backend::cuda::CudaDeviceTable,
) -> AcceleratorDevice {
    let memory_total_bytes = device.memory_info().ok().map(|memory| memory.total);
    let id = device
        .uuid()
        .unwrap_or_else(|_| format!("unavailable-{physical_index}"));
    let pci_bus_id = device.pci_info().ok().map(|pci| pci.bus_id);

    // NVML provides multiprocessor_count via attributes() for MIG/vGPU devices,
    // but returns NotSupported on physical devices under bare-metal drivers.
    // When NVML attributes() is unsupported, fallback to querying the CUDA driver.
    let compute_units = device
        .attributes()
        .ok()
        .map(|attributes| attributes.multiprocessor_count)
        .filter(|&count| count > 0)
        .or_else(|| cuda_table.get_sm_count(&id, pci_bus_id.as_deref(), Some(physical_index)));

    AcceleratorDevice {
        id,
        parent_id,
        display_index: Some(physical_index),
        pci_bus_id,
        vendor: "NVIDIA".to_owned(),
        name: device.name().unwrap_or_else(|_| "NVIDIA GPU".to_owned()),
        architecture: device.architecture().ok().map(|value| format!("{value:?}")),
        entity_kind,
        compute_units,
        memory_total_bytes,
        mig_enabled,
        capabilities: BTreeSet::new(),
    }
}

fn selector_matches(entity: &EntitySpec, selector: &str) -> bool {
    entity.device.id == selector
        || entity
            .device
            .display_index
            .is_some_and(|index| selector == index.to_string())
}

fn capabilities_for(sample: &AcceleratorSample) -> BTreeSet<String> {
    let mut capabilities = BTreeSet::new();
    let candidates = [
        (sample.thermals.temperature_celsius.is_some(), "temperature"),
        (sample.thermals.fan_percent.is_some(), "fan"),
        (sample.power.power_watts.is_some(), "power"),
        (sample.power.power_limit_watts.is_some(), "power_limit"),
        (sample.clocks.sm_clock_mhz.is_some(), "sm_clock"),
        (sample.utilization.gpu_ratio.is_some(), "gpu_utilization"),
        (
            sample.utilization.memory_controller_ratio.is_some(),
            "memory_utilization",
        ),
        (sample.memory.used_bytes.is_some(), "vram"),
        (
            sample.links.pcie_tx_bytes_per_second.is_some(),
            "pcie_throughput",
        ),
        (sample.health.corrected_ecc_volatile.is_some(), "ecc"),
        (sample.health.pcie_replay_counter.is_some(), "pcie_replay"),
    ];
    for (available, name) in candidates {
        if available {
            capabilities.insert(name.to_owned());
        }
    }
    capabilities
}

fn populate_health_observations(
    sample: &mut AcceleratorSample,
    throttle_reasons: Option<ThrottleReasons>,
) {
    let health = &mut sample.health;
    if health
        .uncorrected_ecc_volatile
        .as_ref()
        .is_some_and(|metric| metric.value > 0)
    {
        health
            .observations
            .push("uncorrected volatile ECC errors observed".to_owned());
    }
    if health
        .retired_pages_uncorrected
        .as_ref()
        .is_some_and(|metric| metric.value > 0)
    {
        health
            .observations
            .push("pages retired after uncorrected ECC".to_owned());
    }
    if let Some(reasons) = throttle_reasons {
        if reasons
            .intersects(ThrottleReasons::SW_THERMAL_SLOWDOWN | ThrottleReasons::HW_THERMAL_SLOWDOWN)
        {
            health
                .observations
                .push("thermal slowdown active".to_owned());
        }
        if reasons.contains(ThrottleReasons::HW_POWER_BRAKE_SLOWDOWN) {
            health
                .observations
                .push("hardware power brake active".to_owned());
        }
    }
    if health.observations.is_empty() {
        health
            .observations
            .push("no observed faults in accessible counters".to_owned());
    }
}

fn collect_processes(device: &nvml_wrapper::Device<'_>) -> Vec<ProcessSample> {
    #[derive(Debug)]
    struct ProcessAccumulator {
        process: ProcessSample,
        compute: bool,
        graphics: bool,
    }

    let mut by_pid: BTreeMap<u32, ProcessAccumulator> = BTreeMap::new();
    for (processes, is_compute) in [
        (device.running_compute_processes().unwrap_or_default(), true),
        (
            device.running_graphics_processes().unwrap_or_default(),
            false,
        ),
    ] {
        for info in processes {
            let used_memory = match info.used_gpu_memory {
                UsedGpuMemory::Used(bytes) => Some(bytes),
                UsedGpuMemory::Unavailable => None,
            };
            let entry = by_pid
                .entry(info.pid)
                .or_insert_with(|| ProcessAccumulator {
                    process: ProcessSample {
                        pid: info.pid,
                        kind: if is_compute {
                            ProcessKind::Compute
                        } else {
                            ProcessKind::Graphics
                        },
                        command: process_name(info.pid),
                        // Deliberately excluded from the MVP snapshot: command lines
                        // frequently contain bearer tokens and API keys.
                        command_line: None,
                        used_gpu_memory_bytes: used_memory,
                        sm_ratio: None,
                        memory_ratio: None,
                        encoder_ratio: None,
                        decoder_ratio: None,
                        gpu_instance_id: info.gpu_instance_id,
                        compute_instance_id: info.compute_instance_id,
                    },
                    compute: false,
                    graphics: false,
                });
            entry.compute |= is_compute;
            entry.graphics |= !is_compute;
            entry.process.used_gpu_memory_bytes =
                entry.process.used_gpu_memory_bytes.max(used_memory);
        }
    }

    if let Ok(samples) = device.process_utilization_stats(None::<u64>) {
        for sample in samples {
            let Some(process) = by_pid.get_mut(&sample.pid) else {
                continue;
            };
            process.process.sm_ratio = Some(percent_ratio(sample.sm_util));
            process.process.memory_ratio = Some(percent_ratio(sample.mem_util));
            process.process.encoder_ratio = Some(percent_ratio(sample.enc_util));
            process.process.decoder_ratio = Some(percent_ratio(sample.dec_util));
        }
    }

    by_pid
        .into_values()
        .map(|mut entry| {
            entry.process.kind = match (entry.compute, entry.graphics) {
                (true, true) => ProcessKind::ComputeAndGraphics,
                (true, false) => ProcessKind::Compute,
                (false, true) => ProcessKind::Graphics,
                (false, false) => entry.process.kind,
            };
            entry.process
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn process_name(pid: u32) -> Option<String> {
    fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
}

#[cfg(not(target_os = "linux"))]
fn process_name(_pid: u32) -> Option<String> {
    None
}

fn percent_ratio(value: u32) -> f64 {
    (f64::from(value) / 100.0).clamp(0.0, 1.0)
}

fn pstate_name(state: PerformanceState) -> String {
    let number = match state {
        PerformanceState::Zero => Some(0),
        PerformanceState::One => Some(1),
        PerformanceState::Two => Some(2),
        PerformanceState::Three => Some(3),
        PerformanceState::Four => Some(4),
        PerformanceState::Five => Some(5),
        PerformanceState::Six => Some(6),
        PerformanceState::Seven => Some(7),
        PerformanceState::Eight => Some(8),
        PerformanceState::Nine => Some(9),
        PerformanceState::Ten => Some(10),
        PerformanceState::Eleven => Some(11),
        PerformanceState::Twelve => Some(12),
        PerformanceState::Thirteen => Some(13),
        PerformanceState::Fourteen => Some(14),
        PerformanceState::Fifteen => Some(15),
        PerformanceState::Unknown => None,
    };
    number.map_or_else(|| "P?".to_owned(), |number| format!("P{number}"))
}

fn throttle_reason_names(reasons: ThrottleReasons) -> Vec<String> {
    let labels = [
        (ThrottleReasons::GPU_IDLE, "idle"),
        (
            ThrottleReasons::APPLICATIONS_CLOCKS_SETTING,
            "application clocks",
        ),
        (ThrottleReasons::SW_POWER_CAP, "software power cap"),
        (ThrottleReasons::HW_SLOWDOWN, "hardware slowdown"),
        (ThrottleReasons::SYNC_BOOST, "sync boost"),
        (
            ThrottleReasons::SW_THERMAL_SLOWDOWN,
            "software thermal slowdown",
        ),
        (
            ThrottleReasons::HW_THERMAL_SLOWDOWN,
            "hardware thermal slowdown",
        ),
        (
            ThrottleReasons::HW_POWER_BRAKE_SLOWDOWN,
            "hardware power brake",
        ),
        (
            ThrottleReasons::DISPLAY_CLOCK_SETTING,
            "display clock setting",
        ),
    ];
    labels
        .into_iter()
        .filter(|(flag, _)| reasons.contains(*flag))
        .map(|(_, label)| label.to_owned())
        .collect()
}

fn discover_topology(nvml: &Nvml, entities: &[EntitySpec]) -> Vec<TopologyEdge> {
    let selected_ids = entities
        .iter()
        .map(|entity| entity.device.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut edges = entities
        .iter()
        .filter_map(|entity| {
            let parent = entity.device.parent_id.as_ref()?;
            selected_ids
                .contains(parent.as_str())
                .then(|| TopologyEdge {
                    from: parent.clone(),
                    to: entity.device.id.clone(),
                    kind: TopologyKind::MigParent,
                })
        })
        .collect::<Vec<_>>();

    #[cfg(target_os = "linux")]
    {
        let physical = entities
            .iter()
            .filter(|entity| entity.mig_index.is_none())
            .collect::<Vec<_>>();
        for left_index in 0..physical.len() {
            for right_index in (left_index + 1)..physical.len() {
                let Ok(left) = nvml.device_by_index(physical[left_index].physical_index) else {
                    continue;
                };
                let Ok(right) = nvml.device_by_index(physical[right_index].physical_index) else {
                    continue;
                };
                let kind = left
                    .topology_common_ancestor(right)
                    .map_or(TopologyKind::Unknown, topology_kind);
                edges.push(TopologyEdge {
                    from: physical[left_index].device.id.clone(),
                    to: physical[right_index].device.id.clone(),
                    kind,
                });
            }
        }
    }
    edges
}

#[cfg(target_os = "linux")]
fn topology_kind(level: TopologyLevel) -> TopologyKind {
    match level {
        TopologyLevel::Internal => TopologyKind::PciInternal,
        TopologyLevel::Single => TopologyKind::PciSingleSwitch,
        TopologyLevel::Multiple => TopologyKind::PciMultiSwitch,
        TopologyLevel::HostBridge => TopologyKind::PciHostBridge,
        TopologyLevel::Node => TopologyKind::NumaNode,
        TopologyLevel::System => TopologyKind::System,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_is_a_clamped_ratio() {
        assert_eq!(percent_ratio(0), 0.0);
        assert_eq!(percent_ratio(25), 0.25);
        assert_eq!(percent_ratio(110), 1.0);
    }

    #[test]
    fn pstate_names_are_operator_familiar() {
        assert_eq!(pstate_name(PerformanceState::Zero), "P0");
        assert_eq!(pstate_name(PerformanceState::Fifteen), "P15");
        assert_eq!(pstate_name(PerformanceState::Unknown), "P?");
    }
}
