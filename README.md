# edgar-8k-labels

Nightly **Form 8-K / 8-K/A item labels** from EDGAR. A **label spine** for the mosaic, not a leading observable: Item 2.01 *is* the acquisition disclosure. This crate does not emit a score and does not parse filing bodies.

Production is a systemd oneshot on Linux (`deploy/install.sh`). Capture contract: [docs/CAPTURE.md](docs/CAPTURE.md). Ops: [docs/DAILY_OPS.md](docs/DAILY_OPS.md).

```bash
export SEC_USER_AGENT='edgar-8k-labels you@real-domain'
cargo run --release -- ingest --date 2026-09-11
cargo run --release -- status
cargo run --release -- lookup AAPL
```

`cargo test` uses fixtures only. It does not need the network or a real User-Agent.

Pin `capturable-state` git tag `v0.1.1`. Never `path = "../capturable-state"`.
