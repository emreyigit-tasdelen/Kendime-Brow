# proto

`.proto` schemas — single source for Rust, Go and TypeScript (architecture §3.3).
Rust code is generated at build time by `crates/kb-ipc/build.rs` (prost + protox, no `protoc` install needed).

| File | Contents |
|---|---|
| `kb/ipc/v1/ipc.proto` | Handshake (`Hello`, `HelloAck`) and the generic `Envelope` |
