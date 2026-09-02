# nv-toplike

Real-time NVIDIA GPU telemetry with hardware-grounded terminal visuals.

`nv-toplike` reads NVML directly; it does not spawn `nvidia-smi` in its sampling
loop and does not change device state. The current MVP includes an operational
overview, process attribution, an aggregate SM constellation, a memory-flow
view, PCIe topology, a fleet table, and normalized JSON output.

## Build and run

```bash
cargo build --release
cargo run --release
cargo run --release -- --mode memory
cargo run --release -- --json
```

Controls: `1`–`5` switch views, `Tab` cycles views, arrow keys select a GPU,
and `q` or `Esc` exits.

The default monitor is read-only and does not require root. Unsupported NVML
metrics remain absent or display as `N/A`; they are never replaced with an
unrelated proxy.

See [SPEC.md](SPEC.md) for the product and technical specification.

## Current scope

This is the NVML MVP. DCGM enrichment, remote/inference-service monitoring,
and the opt-in CUPTI trace command remain planned phases. Selecting
`--backend dcgm` currently returns a clear unsupported-backend error.
