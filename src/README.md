# Custom Write-Ahead Log (WAL) Storage Engine

A lightweight, crash-resilient embedded key-value storage engine written from scratch in **Rust**, implementing low-level binary framing and durability guarantees.

## Architecture & Systems Concepts
* **Write-Ahead Logging (WAL):** Prioritizes data durability by serializing mutations into an append-only binary log on disk and forcing a hardware sync (`sync_all()` / `fsync`) before updating in-memory state.
* **Crash Recovery:** Parses the binary log byte-by-byte upon startup, traversing custom frame headers to completely rebuild the in-memory `HashMap` state after an abrupt shutdown or power loss.
* **Zero-Copy Serialization:** Packs commands into strict binary layouts using little-endian byte slicing without unnecessary heap allocations.

## Getting Started

### Prerequisites
* Rust toolchain (`rustc` and `cargo`) installed.

### Running the Simulation
```bash
cargo run