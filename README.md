# nv-toplike

Terminal GPU monitor and performance diagnostics for NVIDIA accelerators.

`nv-toplike` reads NVML directly via low-overhead FFI bindings rather than spawning `nvidia-smi` child processes. It provides real-time telemetry across compute, memory, power, thermals, and interconnect topologies.

---

## Installation

### 1. One-Line Installer (Linux)
Installs the standalone precompiled binary to `~/.local/bin` (or `/usr/local/bin`):

```bash
curl -fsSL https://raw.githubusercontent.com/npow/nv-toplike/main/install.sh | sh
```

### 2. Debian / Ubuntu (`.deb` Package)
Download and install the native `.deb` package from [GitHub Releases](https://github.com/npow/nv-toplike/releases):

```bash
# Example for Ubuntu/Debian x86_64
sudo apt install ./nv-toplike_*_amd64.deb
```

### 3. Cargo (crates.io)
If you have the Rust toolchain installed:

```bash
# Fast prebuilt binary download (no compilation)
cargo binstall nv-toplike

# Or build from crates.io
cargo install nv-toplike
```

### 4. Standalone Binaries (GitHub Releases)
Statically linked, zero-dependency tarballs are available on [GitHub Releases](https://github.com/npow/nv-toplike/releases):
- **Linux x86_64 (musl / static)**: `nv-toplike-v0.1.0-x86_64-unknown-linux-musl.tar.gz`
- **Linux x86_64 (glibc)**: `nv-toplike-v0.1.0-x86_64-unknown-linux-gnu.tar.gz`
- **Linux aarch64 (ARM64 / Jetson / GH200)**: `nv-toplike-v0.1.0-aarch64-unknown-linux-gnu.tar.gz`

---

## Features

- **Direct NVML Telemetry**: Queries the driver directly with sub-millisecond overhead.
- **Engine Utilization Breakdown**: Separates SM compute active time, Tensor Pipe utilization, DRAM controller load, and video encode/decode.
- **SM Core Matrix**: 2D Streaming Multiprocessor grid with temperature-scaled colors and tensor core activity indicators.
- **Memory Hierarchy View**: Physical VRAM allocation mapping, L2 cache crossbar partition activity, and volatile ECC error tracking.
- **Interconnect Topology**: Host-to-device PCIe bus tree, negotiated width/generation, DMA transfer rates, and multi-GPU NVLink/PCIe routing.
- **Process Attribution**: Live GPU process table with per-process VRAM allocation, command line, and compute engine type.
- **JSON Export**: Headless `--json` mode for automated monitoring and metrics ingestion.

---

## Views

`nv-toplike` includes 5 interactive views:

### 1. Overview (`Key: 1`)
Real-time operational overview with core gauges, clock stepping, power headroom, thermal state, and process list.

![Overview View](assets/overview.gif)

---

### 2. SM Constellation (`Key: 2`)
2D grid of all physical Streaming Multiprocessors alongside pipeline telemetry and DMA transfer rates.

![SM Constellation View](assets/constellation.gif)

- **SM Glyphs**: `·` (Idle), `∘` (Low Load), `○` (Moderate), `◉` (Active), `●` (Full Load), `✦` (Spike), `◈` (Tensor Core).
- **Colors**: Dynamic HSV scale mapped to core temperature.

---

### 3. Memory Foundry (`Key: 3`)
Physical memory hierarchy visualization including 2D VRAM allocation block map, L2 cache crossbar partitions, SM register/SRAM vaults, and ECC error counters.

![Memory Foundry View](assets/memory.gif)

---

### 4. Fabric Map (`Key: 4`)
PCIe bus tree topology, link generation/width negotiation, directional DMA bandwidth, and multi-GPU interconnect matrix.

![Fabric Map View](assets/fabric.gif)

---

### 5. Fleet (`Key: 5`)
Multi-GPU cluster overview showing aggregate compute load, total VRAM allocation, total power draw, and per-device status cards.

![Fleet View](assets/fleet.gif)

---

## Controls

| Key | Action |
|---|---|
| `1` | Overview |
| `2` | SM Constellation |
| `3` | Memory Foundry |
| `4` | Fabric Map |
| `5` | Fleet |
| `Tab` | Cycle view |
| `←` / `→` | Select GPU |
| `q` / `Esc` | Quit |

---

## CLI Options

```bash
# Launch directly into a specific view
nv-toplike --mode constellation
nv-toplike --mode memory
nv-toplike --mode fabric
nv-toplike --mode fleet

# Custom refresh rate (FPS)
nv-toplike --fps 30

# Headless single JSON snapshot to stdout
nv-toplike --json

# Run against a mock fixture device (e.g. for testing without NVIDIA hardware)
nv-toplike --fixture blackwell-rtx6000
```

---

## Building from Source

Requirements: NVIDIA Display Driver (450.00+) and Rust 1.88+.

```bash
git clone https://github.com/npow/nv-toplike.git
cd nv-toplike
cargo build --release
./target/release/nv-toplike
```

---

## License

Apache-2.0. See [LICENSE](LICENSE) for details.
