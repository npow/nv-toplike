# Hardware validation log

## 2026-09-02 — Blackwell workstation GPU

Environment:

- GPU: NVIDIA RTX PRO 6000 Blackwell Max-Q Workstation Edition
- Driver: 610.43.02
- NVML: 13.610.43.02
- OS: Ubuntu 26.04, Linux 7.0.0-30-generic
- Build: Rust 1.97.1, release binary 1.6 MiB
- State: idle desktop with one graphics process

The GPU UUID was deliberately omitted from this log and replaced by a
synthetic value in the committed fixture.

### Reference comparison

One `nvidia-smi --query-gpu` sample and one direct NVML `nv-toplike --json`
sample were taken consecutively:

| Metric | `nvidia-smi` | `nv-toplike` normalized JSON |
|---|---:|---:|
| GPU utilization | 0% | 0% |
| Memory utilization | 0% | 0% |
| VRAM used | 241 MiB | 240.75 MiB |
| VRAM total | 97,887 MiB | 97,887 MiB |
| Power draw | 5.61 W | 5.609 W |
| Enforced power limit | 300.00 W | 300.0 W |
| GPU temperature | 33 °C | 33.0 °C |
| SM clock | 180 MHz | 180 MHz |
| Memory clock | 405 MHz | 405 MHz |
| PCIe current generation | 1 | 1 |
| PCIe current width | 16 | 16 |

The sub-MiB VRAM difference is formatting precision: NVML reports bytes and
the reference command rounded to whole MiB.

### Capability findings

Available on this system:

- GPU and memory-controller utilization;
- VRAM allocation;
- power draw and enforced limit;
- temperature and fan percentage;
- graphics, SM, memory, and video clocks;
- performance state and throttle reasons;
- PCIe generation, width, throughput, and replay counter;
- compute/graphics process enumeration and process VRAM;
- encoder/decoder utilization.

Unavailable or not applicable:

- `nvmlDeviceGetAttributes_v2` did not expose a physical SM count on this SKU;
  the Constellation therefore uses 64 explicitly labeled display cells rather
  than deriving a false SM count from CUDA-core marketing totals;
- MIG mode is not applicable on this workstation configuration;
- volatile ECC and retired-page calls were unsupported, so those fields remain
  absent rather than displaying zero;
- DCGM was not installed, so SM-active, occupancy, tensor, DRAM-active, and
  NVLink enrichment were not tested.

### Behavioral validation

- All five views rendered in a real 80×24 PTY and exited cleanly with `q`.
- Renderer tests cover every view at 80×24 and 120×40 without NVIDIA hardware.
- Display-index and exact-UUID selectors returned the same physical device.
- An unknown selector returned a non-zero, human-readable error.
- Explicit DCGM selection returned a non-zero unsupported-backend error.
- The collector samples off the UI thread and retains the last good snapshot.

