# nv-toplike Product and Technical Specification

Status: NVML MVP implemented; later enrichment phases remain planned  
Created: 2026-09-02  
Working name: `nv-toplike`

## Implementation status

The repository currently implements the safe NVML operational and visual MVP:

- direct NVML collection with no `nvidia-smi` subprocess in the sample loop;
- normalized, provenance-carrying JSON snapshots;
- UUID-stable physical GPU selection and MIG discovery/filtering;
- power, temperature, fan, clock, utilization, VRAM, PCIe, ECC, retirement,
  replay, throttle, and process telemetry when supported;
- a non-blocking collector that retains and marks the last good sample;
- Overview, SM Constellation, Memory Foundry, Fabric Map, and Fleet views;
- adaptive per-device visual baselines and explicit aggregate/illustrative
  labels;
- hardware-free unit, renderer, schema round-trip, and sanitized fixture tests;
- live validation on an RTX PRO 6000 Blackwell against driver 610.43.02.

The DCGM enrichment, inference-service/remote layers, and opt-in CUPTI tracer
remain later phases in this specification. The CLI rejects `--backend dcgm`
clearly until that provider exists.

## 1. Summary

`nv-toplike` is a real-time terminal and optional desktop monitor for NVIDIA
GPUs. It applies the visual language developed in `tt-toplike`—adaptive
baselines, animated hardware views, fleet layouts, process attribution, and
inference-service monitoring—to NVIDIA hardware without presenting inferred
activity as directly measured physical detail.

The project is a sibling of `tt-toplike`, not an NVIDIA mode inside the
Tenstorrent-branded program. Reusable rendering and monitoring code may later
move into a shared crate after the NVIDIA implementation proves which
abstractions are genuinely common.

The safe default backend uses NVML directly. DCGM is optional enrichment for
device-level profiling and fabric health. CUPTI is an explicitly opt-in tracing
facility and is never enabled by automatic backend selection.

## 2. Product goals

1. Make GPU activity legible at a glance, especially changes between idle,
   model loading, prefill, decode, diffusion, training, and peer transfers.
2. Work on a normal NVIDIA driver installation without requiring root, a
   daemon, CUDA toolkit installation, or changes to the monitored process.
3. Scale from one consumer GPU to multi-GPU NVLink/NVSwitch servers and MIG
   deployments.
4. Attribute GPU memory and activity to processes when the driver and hardware
   expose that data.
5. Reuse the strongest `tt-toplike` interaction and visualization ideas while
   using NVIDIA-accurate terminology and metric semantics.
6. Degrade visibly and honestly when a metric is unsupported, permission-gated,
   stale, or unavailable.
7. Keep passive monitoring low-overhead and safe for production workloads.

## 3. Non-goals

- Replacing Nsight Systems, Nsight Compute, DCGM Diagnostics, or NVIDIA field
  diagnostics.
- Claiming per-SM activity when only a GPU-wide aggregate is available.
- Claiming cache-line, L2-to-L1, register, or physical memory-channel traffic
  from management telemetry.
- Injecting a profiler into arbitrary CUDA processes in the default mode.
- Changing clocks, power limits, compute modes, MIG configuration, persistence
  mode, or any other device state.
- Running stress tests or active diagnostics during normal monitoring.
- Treating allocation (`VRAM used`) as traffic (`DRAM active` or bytes/sec).

## 4. Design principles

### 4.1 Truthful animation

Every animation driver must have a documented metric source and semantic. A
spatial visualization may be illustrative, but its labels and help text must
state the aggregation level.

Examples:

- A grid of SM glyphs may brighten together from aggregate SM activity.
- It must not assign different activity values to individual SMs unless a
  future source actually supplies per-SM samples.
- Memory particles may represent measured PCIe/NVLink bytes or aggregate DRAM
  activity.
- They must not be described as observed cache lines.

### 4.2 Capability-driven UI

Availability is represented explicitly. Unsupported metrics are absent or
marked `N/A`; they are never silently synthesized from unrelated values.
Views declare required and optional capabilities and adapt their labels and
layout accordingly.

### 4.3 Safe by default

Automatic detection selects only passive NVML collection. DCGM profiling and
CUPTI tracing require explicit selection and display their permission,
compatibility, and observer-effect implications.

### 4.4 Stable identity

GPU identity is keyed by UUID. Display index and PCI address are attributes,
not durable identifiers. MIG instances use their MIG UUID plus parent GPU UUID.

### 4.5 Adaptive baselines

Visual intensity combines absolute utilization with change relative to a
learned idle baseline. Baselines are per physical GPU or per MIG entity and are
reset after device loss, driver reset, suspend/resume, or a large monotonic-time
gap.

## 5. Telemetry sources

### 5.1 NVML backend (required, default)

Use the NVML shared library through a Rust wrapper or a small internal FFI
layer. Do not poll by spawning `nvidia-smi` in the steady-state update loop.
`nvidia-smi` may be used only as a diagnostic fallback or compatibility probe.

Collect when supported:

- driver version and NVML version;
- GPU name, UUID, serial number, architecture where discoverable, PCI bus ID;
- physical memory total, used, free, and reserved;
- GPU utilization and memory-controller utilization;
- board/module power, enforced power limit, and total energy where available;
- GPU temperature, thresholds, and fan speed;
- graphics/SM and memory clocks, P-state, and clock-event/throttle reasons;
- PCIe generation, width, replay/error information, and throughput where
  available;
- encoder, decoder, JPEG, and optical-flow utilization where available;
- volatile and aggregate ECC counts, retired pages, and row-remap state;
- compute/graphics processes, PID, used device memory, and recent per-process
  utilization when supported;
- NVLink state/counters exposed by the installed NVML version;
- topology relationships between physical GPUs;
- MIG mode, GPU instances, compute instances, profiles, UUIDs, and parentage.

An unsupported NVML call is normal capability discovery, not a backend failure.
The backend fails only when NVML cannot initialize or no visible GPU/MIG entity
can be enumerated.

### 5.2 DCGM backend/enricher (optional)

DCGM is an optional provider layered over the base device model. It may connect
to an existing `nv-hostengine` or use embedded mode where supported.

Preferred profiling fields include:

- graphics engine activity;
- SM active and SM occupancy;
- tensor, FP16, FP32, and FP64 pipeline activity;
- DRAM active;
- PCIe transmit/receive bytes per second;
- NVLink transmit/receive bytes per second;
- framebuffer use and standard device telemetry;
- XID, ECC, PCIe replay, NVLink, NVSwitch, and fabric health fields.

Requirements:

- `--backend dcgm` or `--enrich dcgm` is explicit;
- report whether collection is embedded or host-engine based;
- report permission failures separately from unsupported fields;
- detect blank/paused profiling watches;
- expose metric-group conflicts or multiplexing rather than substituting zero;
- never start active DCGM diagnostics from the monitoring loop;
- document that profiler counters can conflict with developer profiling tools.

### 5.3 CUPTI trace provider (future, opt-in)

CUPTI supports an application-cooperative or explicitly launched trace mode for
kernel, memory-copy, synchronization, and CUDA API activity.

Requirements:

- never selected automatically;
- separate command or explicit `--trace` mode;
- clear observer-effect warning;
- distinguish host-to-device, device-to-host, device-to-device, and peer-copy
  byte counts;
- retain dropped-record counts and show when trace buffers overflow;
- never translate CUPTI records into per-SM state unless the selected CUPTI API
  genuinely supplies it.

## 6. Core data model

The model is vendor-aware but renderer-neutral. All non-identity telemetry
fields are optional and carry source/age metadata.

```rust
struct AcceleratorDevice {
    id: DeviceId,                  // UUID-based stable identity
    parent_id: Option<DeviceId>,   // MIG -> physical GPU
    display_index: Option<u32>,
    pci_bus_id: Option<String>,
    vendor: Vendor,                // Nvidia
    name: String,
    architecture: Option<String>,  // e.g. Ampere, Hopper, Blackwell
    entity_kind: EntityKind,       // PhysicalGpu, MigGpuInstance, MigComputeInstance
    compute_units: Option<u32>,    // SM count or assigned SM count
    memory_kind: Option<String>,   // GDDR6X, HBM3, etc., only when known
    memory_total_bytes: Option<u64>,
    capabilities: Capabilities,
}

struct AcceleratorSample {
    captured_at: SystemTime,
    monotonic_at: Instant,
    thermals: ThermalSample,
    power: PowerSample,
    clocks: ClockSample,
    utilization: UtilizationSample,
    memory: MemorySample,
    links: LinkSample,
    health: HealthSample,
}

struct Metric<T> {
    value: T,
    source: MetricSource,          // Nvml, Dcgm, Cupti, Prometheus
    scope: MetricScope,            // Device, MigInstance, Process, Link, Workload
    sampled_at: SystemTime,
    quality: MetricQuality,        // Direct, Derived, Estimated
}
```

Important fields:

- `gpu_utilization_ratio`
- `memory_controller_utilization_ratio`
- `sm_active_ratio`
- `sm_occupancy_ratio`
- `tensor_active_ratio`
- `dram_active_ratio`
- `vram_used_bytes`, `vram_total_bytes`
- `power_watts`, `power_limit_watts`, `energy_millijoules`
- `temperature_celsius`, `temperature_limit_celsius`
- `graphics_clock_mhz`, `sm_clock_mhz`, `memory_clock_mhz`
- `pcie_tx_bytes_per_second`, `pcie_rx_bytes_per_second`
- `nvlink_tx_bytes_per_second`, `nvlink_rx_bytes_per_second`
- `encoder_ratio`, `decoder_ratio`, `jpeg_ratio`, `ofa_ratio`
- ECC, XID, retired-page, row-remap, PCIe replay, and fabric-health state

Ratios are stored as `0.0..=1.0`. Rates use bytes per second. Storage uses
bytes. UI formatting chooses SI/IEC units consistently and labels them.

## 7. Sampling and state

- Default UI rate: 10 frames/sec.
- NVML sampling rate: up to 5–10 Hz for cheap gauges, bounded by the actual
  sampling period reported or implied by the API.
- DCGM profiling default: 1 Hz; allow configuration down to supported minimums.
- Expensive/static identity and topology probes: startup plus slow refresh.
- Process discovery: 1 Hz by default.
- Prometheus workload scraping: 1 Hz by default.
- Samples older than `3 * expected_interval` are marked stale.
- Counter rates use monotonic time and handle reset/wrap by dropping one delta.
- No UI animation loop may block on driver calls. Collection runs separately
  and publishes immutable snapshots.

## 8. Views

### 8.1 Overview

The default operational screen contains:

- one card per physical GPU or selected MIG entity;
- GPU/SM utilization;
- VRAM used/total and memory activity;
- power/current limit percentage, temperature, clocks, and P-state;
- PCIe and NVLink traffic where available;
- active processes with PID, command, memory, and utilization;
- health events and throttle reasons;
- inference-service attribution where available.

### 8.2 SM Constellation

Adapt the `tt-toplike` Starfield aesthetic to NVIDIA terminology.

- Geometry uses the device's SM count, arranged for display rather than
  claiming physical floorplan accuracy.
- With only NVML, all glyphs respond to aggregate GPU utilization plus the
  adaptive baseline.
- With DCGM, brightness may combine SM active, occupancy, and tensor activity.
- Tensor activity changes glyph/style, not fictitious SM-local values.
- Header must say `aggregate activity across N SMs`.

### 8.3 Memory Foundry

Adapt Memory Castle to the NVIDIA memory hierarchy:

```text
Host memory <-> PCIe/NVLink <-> VRAM/HBM <-> L2 <-> SM L1/shared/registers
```

- VRAM fill controls the persistent reservoir level.
- NVML memory-controller utilization or DCGM DRAM active controls memory-layer
  motion.
- PCIe and NVLink byte rates drive directional particles at their boundaries.
- CUPTI copy records, when explicitly tracing, may drive typed H2D/D2H/D2D/P2P
  particles.
- L2 and SM-local layers are architectural context unless a direct metric is
  present. Help text must state that internal movement is illustrative.
- Do not invent physical memory channels or per-channel temperatures.

### 8.4 Fabric Map

- Render physical GPUs, MIG children, PCIe relationships, NUMA affinity,
  NVLink edges, and NVSwitch entities when available.
- Edge thickness/animation uses measured bytes/sec.
- Link state and errors override decorative color with explicit warning state.
- A topology relationship without a traffic counter is rendered static.

### 8.5 Fleet Grid

- Scale to at least 64 physical GPUs.
- Default compact cell: utilization, temperature, power ratio, VRAM ratio,
  health state.
- Stable ordering by PCI bus ID, with optional UUID/index/name sort.
- MIG children can be collapsed beneath their parent.

### 8.6 Workload/Inference view

The workload layer is hardware-vendor independent and should reuse the serving
semantics already proven in `tt-toplike`:

- discover known runtimes from process command lines;
- scrape configured or discovered Prometheus endpoints;
- support vLLM request/token/latency metrics;
- support diffusion/media-server generation, duration, queue, and in-flight
  metrics;
- never infer tokens/sec from GPU utilization;
- distinguish hardware activity from application throughput.

## 9. Health semantics

Replace Tenstorrent-specific ARC and DDR-training concepts with NVIDIA-native
health signals:

- latest XID and time observed;
- volatile/aggregate corrected and uncorrected ECC changes;
- row-remap failures and pending retirements;
- PCIe replay/error deltas;
- NVLink link-down, recovery, CRC/BER, and fabric-health events;
- thermal, power, reliability, and synchronization clock-event reasons;
- device-lost and inaccessible states;
- DCGM passive health state when enabled.

The passive monitor must not label a device globally `healthy` merely because
no accessible counter reports an error. Use `no observed faults` when coverage
is partial.

## 10. Process attribution

Each process record may include:

- PID, user, executable, command line, container/cgroup identity;
- physical GPU and MIG UUID;
- compute/graphics/MPS mode;
- device memory usage;
- SM, memory, encoder, decoder, JPEG, and OFA utilization when available;
- associated listening ports and recognized inference runtime;
- application metrics from an attributed service endpoint.

MPS can obscure client attribution; the UI must state when activity is visible
only at the MPS server or device level.

## 11. CLI contract

Initial commands and flags:

```text
nv-toplike
nv-toplike --backend nvml
nv-toplike --backend dcgm
nv-toplike --enrich dcgm
nv-toplike --device GPU-<uuid>
nv-toplike --device 0
nv-toplike --mig physical|instances|all
nv-toplike --mode overview|constellation|memory|fabric|fleet|inference
nv-toplike --fps 10
nv-toplike --sample-ms 200
nv-toplike --json
nv-toplike --serve [ADDR:PORT]
nv-toplike --remote HOST:PORT
nv-toplike trace -- <command> [args...]
```

`--json` emits the normalized schema, not raw `nvidia-smi` output. Include
schema version, capability set, source per metric, timestamp, and staleness.

## 12. Remote protocol

- Versioned normalized snapshots over WebSocket.
- Stable UUID identities; never rely on remote display indexes.
- Additive schema evolution with defaults for absent optional fields.
- Optional process and workload extensions.
- Plaintext serving is allowed only as an explicit trusted-LAN mode and must
  bind to loopback by default.
- Do not expose environment variables or unredacted secrets from command lines.

## 13. Project structure

Proposed Rust workspace:

```text
nv-toplike/
  Cargo.toml
  crates/
    nv-toplike-core/       normalized model, sampling, baselines
    nv-toplike-nvml/       safe default backend
    nv-toplike-dcgm/       optional enrichment/backend
    nv-toplike-cupti/      future opt-in tracer
    nv-toplike-ui/         ratatui views and visualization engine
    nv-toplike-workloads/  process/service/inference monitoring
    nv-toplike-remote/     normalized wire protocol
  src/bin/
    nv-toplike.rs
```

Do not extract a shared crate from `tt-toplike` before the first NVIDIA MVP.
Initially copy the Apache-2.0-compatible reusable pieces with provenance. After
both implementations have working hardware tests, compare them and extract
only stable, semantically common code.

## 14. Safety and performance requirements

- Default monitoring is read-only.
- No root requirement for NVML mode.
- No automatic persistence-mode or driver-setting changes.
- No active DCGM diagnostics from the UI.
- No CUPTI initialization outside explicit trace mode.
- Target steady-state monitor CPU: below 2% of one host core at 10 FPS on an
  otherwise idle system, excluding terminal cost and optional profilers.
- Target UI input-to-render latency: below 100 ms.
- A failed device query must not stall or remove healthy devices.
- Backoff repeated driver errors and preserve the last sample as visibly stale.

## 15. Testing strategy

### Unit tests

- capability negotiation and unsupported NVML return codes;
- UUID-based identity and sparse/display-index changes;
- counter delta, reset, wrap, and stale-sample behavior;
- adaptive baseline normalization;
- metric source/scope/quality propagation;
- MIG parent/child topology;
- layout width using Unicode display columns;
- absence of right-side TUI borders if retaining the `tt-toplike` convention;
- backward-compatible remote schema decoding.

### Fixture tests

- recorded normalized snapshots for consumer, workstation, and data-center
  GPUs;
- single and multi-GPU PCIe systems;
- NVLink and NVSwitch systems;
- MIG enabled/disabled and multiple instance profiles;
- permission-denied DCGM profiling;
- unsupported fan/ECC/process-utilization calls;
- driver reset/device-lost recovery;
- MPS and containerized processes.

Fixtures must be sanitized and must not contain hostnames, usernames, commands
with secrets, or stable machine identifiers beyond synthetic replacements.

### Hardware validation

At least one device from each available class before a stable release:

- consumer GeForce;
- workstation RTX;
- data-center GPU;
- multi-GPU NVLink or NVSwitch system;
- MIG-capable system.

Validate displayed values against a simultaneous NVML/DCGM reference stream,
not screenshots alone.

## 16. Delivery phases

### Phase 0: Probe

- Enumerate live NVML capabilities on available NVIDIA systems.
- Capture sanitized fixture snapshots.
- Confirm Rust binding/library compatibility and redistribution constraints.

Exit criterion: a capability matrix for at least two materially different GPUs.

### Phase 1: NVML operational MVP

- Workspace, normalized model, NVML backend, collector thread.
- Overview, process roster, fleet grid, JSON output.
- Power, temperature, clocks, utilization, VRAM, PCIe identity, health basics.

Exit criterion: stable read-only operation for one hour during idle and a CUDA
workload, including workload exit and restart.

### Phase 2: Visual MVP

- SM Constellation and Memory Foundry.
- Adaptive baselines, legends, help, and truthful aggregation labels.
- Multi-GPU layout.

Exit criterion: every animated variable is documented and traceable to a field
in the normalized sample.

### Phase 3: Fabric and MIG

- Physical/MIG hierarchy, PCIe/NVLink topology, link traffic and state.
- Fabric Map and collapsible fleet hierarchy.

Exit criterion: correct identity and attribution through MIG reconfiguration
after an explicit refresh/restart; no index-based misattribution.

### Phase 4: DCGM enrichment

- SM/tensor/DRAM profiling fields and passive health.
- Permission and profiler-conflict reporting.

Exit criterion: clean fallback to NVML-only mode when DCGM is absent, paused,
unsupported, or unauthorized.

### Phase 5: Workloads and remote monitoring

- Inference/service metrics, WebSocket publisher/client, discovery if desired.

Exit criterion: hardware and application metrics remain separately sourced and
correctly attributed across the wire.

### Phase 6: CUPTI trace mode

- Launched-process kernel and copy timeline.

Exit criterion: explicit overhead disclosure, dropped-record reporting, and no
behavior change to ordinary monitoring commands.

## 17. MVP acceptance criteria

The first public MVP is complete when:

1. It discovers all NVML-visible physical GPUs without spawning `nvidia-smi` in
   the sampling loop.
2. It shows GPU utilization, memory utilization, VRAM, power, temperature,
   clocks, process memory, and supported health state with source-aware `N/A`
   handling.
3. It handles multiple GPUs and sparse/changing display indexes using UUIDs.
4. Its two animated views label aggregate versus direct measurements correctly.
5. It survives process churn and transient NVML errors without exiting.
6. It performs no device mutations and requires no elevated privileges in the
   default mode.
7. Unit and fixture tests run without NVIDIA hardware.
8. A hardware validation log records GPU model, driver/NVML versions, supported
   capabilities, reference commands/tools, and observed discrepancies.

## 18. Open decisions

- Final project and binary name; `nv-toplike` is provisional and may need a
  trademark/name review.
- Exact Rust NVML wrapper versus maintained internal FFI surface.
- Whether the first release is TUI-only or includes the egui application.
- Whether remote discovery belongs in the first public release.
- Minimum supported NVIDIA driver/NVML version.
- Which DCGM installation modes and versions are supportable across consumer
  and data-center distributions.
- Whether common code eventually becomes a neutral `accelerator-toplike-core`
  workspace shared with `tt-toplike`.

## 19. Upstream references

- NVML API: <https://docs.nvidia.com/deploy/nvml-api/>
- `nvidia-smi`: <https://docs.nvidia.com/deploy/nvidia-smi/>
- DCGM: <https://docs.nvidia.com/datacenter/dcgm/latest/>
- DCGM profiling: <https://docs.nvidia.com/datacenter/dcgm/latest/learn/modules/profiling.html>
- MIG guide: <https://docs.nvidia.com/datacenter/tesla/mig-user-guide/>
- CUPTI: <https://docs.nvidia.com/cupti/>
