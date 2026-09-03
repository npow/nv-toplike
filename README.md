# nv-toplike

**Real-time NVIDIA GPU telemetry with hardware-grounded terminal visuals.**

`nv-toplike` is a high-density, low-overhead GPU monitoring dashboard and diagnostic tool built in Rust. It interacts with the NVIDIA Management Library (**NVML**) directly without spawning subshells or polling `nvidia-smi`, providing sub-millisecond hardware telemetry, bottleneck detection, memory hierarchy mapping, and cluster observability.

---

## Key Features

- **Direct NVML Integration**: Read-only, unprivileged, low-overhead hardware telemetry without child process overhead.
- **Bottleneck Identification**: Real-time breakdown of **SM Compute**, **Tensor Pipe**, **DRAM Controller**, and **PCIe Direct DMA** transfer rates.
- **2D SM Compute Matrix**: Responsive hardware-anchored SM core galaxy with dynamic HSV temperature gradient and tensor pipe detection.
- **Full Memory Hierarchy**: Multi-tier VRAM allocation reservoir, L2 cache crossbar partitions, SM local SRAM vaults, and volatile ECC error tracking.
- **PCIe & NVLink Fabric**: Host-to-device bus tree diagram, link generation/width negotiation, replay counters, and inter-GPU topology queries.
- **Zero Hallucination / Proxying**: If a metric is unsupported by hardware or driver, it displays as `N/A` rather than faking data with proxy numbers.
- **Machine-Readable JSON**: `--json` headless output format for automated monitoring and data pipelines.

---

## Visual Views & Walkthrough

`nv-toplike` features 5 dedicated views tailored for development, training, and cluster management.

### View 1: Overview (`Key: 1`)
*The primary operational cockpit providing at-a-glance hardware telemetry, clock stepping, and process attribution.*

```text
╭ Overview ────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ NVIDIA RTX PRO 6000 Blackwell · Blackwell (188 SMs) · Driver 610.43.02 · GPU 0/1                                     │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭ GPU Core Load ────────╮╭ VRAM Allocation ─────╮╭ Board Power ─────────╮╭ GPU Thermal ─────────╮
│ ▓▓▓▓▓▓▓▓▓▓░░░░░░  52% ││ ▓▓▓▓▓░░░░░░░░░░  25% ││ ▓▓▓▓▓▓▓▓░░░░░░░ 142W ││ ▓▓▓▓▓░░░░░░░░░░  54°C │
│ Baseline Activity 48% ││ 24.0GiB / 96.0GiB    ││ Cap: 300W · Headroom ││ Fan: 45% · Nominal   │
╰───────────────────────╯╰──────────────────────╯╰──────────────────────╯╰──────────────────────╯
╭ Hardware & Clocks ────────────────────╮╭ Framebuffer & Controller ────────────╮╭ Engine & Direct DMA ────────────────╮
│ SM Clock:      2520 MHz · P0          ││ Allocated:     24.0GiB (25.0%)       ││ Tensor Core:   Active (78%) ◈       │
│ Memory Clock: 14000 MHz               ││ Free:          72.0GiB               ││ Video Enc/Dec: ENC 0% · DEC 0%      │
│ Bus Link:     PCIe Gen5 x16           ││ Bandwidth:     1480.0 GB/s           ││ Host TX (D2H): ──────────▶ 2.4GiB/s │
│ Throttle:     None (Nominal)          ││ ECC State:     0 corr · 0 uncorr     ││ Host RX (H2D): ◀────────── 8.1GiB/s │
╰───────────────────────────────────────╯╰──────────────────────────────────────╯╰──────────────────────────────────────╯
╭ Active GPU Compute Processes ────────────────────────────────────────────────────────────────────────────────────────╮
│   PID     PROCESS NAME         COMMAND LINE                         VRAM USAGE    GPU ENGINE    CONTEXT TYPE         │
│   412940  python3              vllm serve meta-llama/Llama-3-70B    22.4GiB       Compute       CUDA Primary         │
│   413812  triton-server        tritonserver --model-repository=/…   1.2GiB        Compute       CUDA Primary         │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

---

### View 2: SM Constellation (`Key: 2`)
*Dynamic 2D Streaming Multiprocessor compute matrix coupled with live pipeline telemetry.*

```text
╭ SM Constellation · Aggregate Compute Galaxy ─────────────────────────────────────────────────────────────────────────╮
│ NVIDIA RTX PRO 6000 · 188 SMs · Arch: Blackwell · GPU Load: 52% · Baseline: 48% · Tensor Pipe: 78% ◈                  │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭ 2D SM Matrix · 188 Units (6 Clusters/Row) ─────────────────────────────────╮╭ Pipeline & Engine Activity ────────────╮
│SM000│ [ ✦ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ]││SM Compute    52% [Baseline  48%]       │
│SM024│ [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ✦ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ]││Tensor Core   78% [Active ◈]            │
│SM048│ [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ✦ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ]││DRAM Ctrl     34% [Active ◆]            │
│SM072│ [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ]││Video Engine ENC:   0% · DEC:   0%      │
│SM096│ [ ∘ ∘ ✦ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ]│╰────────────────────────────────────────╯
│SM120│ [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ✦ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ]│╭ Clocks & Power Dynamics ───────────────╮
│SM144│ [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ]││Clocks: SM 2520MHz · MEM 14000MHz · P0 │
│SM168│ [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ∘ ] [ ∘ ∘ ∘ ✦ ]          ││Power:  142.0W / 300.0W                │
╰────────────────────────────────────────────────────────────────────────────╯│Thermal: 54°C · Fan 45%                  │
╭ Memory Foundry · VRAM Reservoir & L2 Crossbar ─────────────────────────────╮╰────────────────────────────────────────╯
│◆▓▓▓▓▓▓▓▓▓▓▓▓◆▓▓▓▓░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │╭ PCIe DMA Highway & Process Attribution ╮
│▓▓▓◆▓▓▓▓▓▓▓▓▓▓▓▓◆▓░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ ││Host TX (D2H)  ─────────────▶   2.4GiB/s│
│VRAM: 24.0GiB / 96.0GiB (25.0%) · Free: 72.0GiB                             ││Host RX (H2D)  ◀─────────────   8.1GiB/s│
│[L2_00:◇] [L2_01:◆] [L2_02:◇] [L2_03:◆] [L2_04:◆] [L2_05:◇] [L2_06:◆]       ││Bus: PCIe Gen5 x16 · Replays: 0         │
│[L2_07:◆] [L2_08:◇] [L2_09:◆] [L2_10:◆] [L2_11:◇] [L2_12:◆] [L2_13:◇]       ││Processes: 2 active compute jobs        │
╰────────────────────────────────────────────────────────────────────────────╯╰────────────────────────────────────────╯
```

- **SM Glyphs**: `·` (Idle), `∘` (Low Load), `○` (Moderate), `◉` (Active), `●` (Full Load), `✦` (Spike), `◈` (Tensor Core).
- **HSV Palette**: Dynamic core temperature gradient (Cyan 35°C → Neon Green 50°C → Amber 70°C → Hot Red 85°C).

---

### View 3: Memory Foundry (`Key: 3`)
*Deep-dive physical memory hierarchy from on-chip SRAM to physical DRAM and ECC health.*

- **2D VRAM Allocation Map**: Granular memory block reservoir (`▓` allocated, `░` free) with active memory controller packets (`◆`).
- **L2 Cache Crossbar**: 16 partitioned crossbar slices reflecting cache traffic.
- **SM Local Execution Vaults**: 8-cluster SRAM/register array representing L1 data cache and register files.
- **Hardware & Reliability Deck**: Controller clocks, memory bus bandwidth, volatile ECC error tracking, and page retirements.

---

### View 4: Fabric Map (`Key: 4`)
*Host-to-device PCIe tree topology, NUMA node binding, and inter-GPU interconnect matrix.*

- **Bus Tree**: Displays Root Complex, PCIe Switch bridges, link generation/width (`Gen5 x16`), and DMA data channels.
- **Multi-GPU Topology**: Identifies direct NVLink, PCIe Host Bridge, Single Switch (PIX), or NUMA cross-socket routing.
- **Fixed-Width Link Channels**: Directional transfer speed counters (`Host TX ──▶` and `Host RX ◀──`) formatted with strict character width to eliminate visual jitter.

---

### View 5: Fleet (`Key: 5`)
*Multi-GPU cluster overview and node distribution.*

- **Cluster Summary**: Aggregate cluster compute load, total VRAM consumption, cluster board power draw, and average thermal profile.
- **Device Grid**: Compact GPU status cards with individual load and memory gauges.

---

## Installation & Build

### Prerequisites
- NVIDIA GPU with driver installed (NVIDIA Display Driver 450.00+ or later).
- Rust 1.80+ and Cargo toolchain.

### Building from Source

```bash
git clone https://github.com/npow/nv-toplike.git
cd nv-toplike

# Build optimized release binary
cargo build --release

# Run interactive TUI
./target/release/nv-toplike
```

---

## Usage & Controls

### Keyboard Navigation

| Key | Action |
|---|---|
| `1` | **Overview**: Operational gauges, clocks, power, processes, and diagnostics |
| `2` | **SM Constellation**: 2D SM galaxy, tensor pipe, and pipeline telemetry |
| `3` | **Memory Foundry**: VRAM reservoir map, L2 cache crossbar, and ECC health |
| `4` | **Fabric Map**: PCIe tree, DMA transport highway, and topology |
| `5` | **Fleet**: Multi-GPU cluster summary and per-device status cards |
| `Tab` | Cycle sequentially through all views |
| `←` / `→` | Switch active GPU on multi-GPU systems |
| `q` / `Esc` | Quit `nv-toplike` |

### CLI Options

```bash
# Launch directly into a specific view
nv-toplike --mode constellation
nv-toplike --mode memory
nv-toplike --mode fabric
nv-toplike --mode fleet

# Custom refresh rate (frames per second)
nv-toplike --fps 30

# Headless JSON export (single snapshot to stdout)
nv-toplike --json

# Run against a mock fixture device
nv-toplike --fixture blackwell-rtx6000
```

---

## Troubleshooting & Diagnostics

- **Missing Metrics / `N/A`**: Consumer or workstation GeForce cards without ECC or aggregate power sensors will display `N/A` for those specific fields. This is intentional to ensure zero metric hallucination.
- **Permission Requirements**: `nv-toplike` is read-only and runs under any standard unprivileged user account. Root / `sudo` access is **not** required.

---

## License

Apache-2.0. See [LICENSE](LICENSE) for details.
