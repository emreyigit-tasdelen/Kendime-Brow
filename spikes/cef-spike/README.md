# CEF spike (Phase 1.1 — V1, V3, V4)

Minimal single-window browser using the Rust [`cef`](https://crates.io/crates/cef) crate
(`=156.4.0`, Chromium 156). Based on the `cefsimple` example from `tauri-apps/cef-rs`
(Apache-2.0 / MIT). Throwaway code: the real shell starts in task 1.3.

## Build and run (Windows)

Prerequisites: Rust stable, Visual Studio 2022 Build Tools (MSVC), CMake, Ninja
(`winget install Kitware.CMake Ninja-build.Ninja`; make sure both are on `PATH`).

```sh
cargo build                     # downloads the CEF binary distribution (~2.2 GB target dir)
./target/debug/cef-spike.exe --url=https://example.org
./target/debug/cef-spike.exe --use-alloy-style --url=...
```

The build script of `cef-dll-sys` downloads CEF and copies `libcef.dll`, resources, locales and
`bootstrap.exe` next to the executable. Without the `sandbox` feature the app runs unsandboxed;
the Windows sandbox requires launching through `bootstrap.exe` (to be done in the real shell).

## Results (2026-11-12, Windows 11)

| # | Assumption | Result |
|---|---|---|
| V1 | Official CEF binaries lack H.264/AAC | **Confirmed.** `test/codecs.html`: h264=no, aac=no, hevc=no; vp9, av1, opus, mp3 = probably |
| V3 | Chrome style and Alloy style both work | **Confirmed** for each style separately (Views window, `--use-alloy-style`). Mixing both in one app is checked in 1.6 |
| V4 | The `cef` crate covers the needed APIs | **Confirmed.** Views, `ResourceRequestHandler::on_before_resource_load`, `send_dev_tools_message` + observer, scheme handler factory, request context, render process handler / `on_context_created`, preferences, `CEF_CTT_NONE`, and a Rust port of the message router (`wrapper/message_router.rs`). No C++ bridge needed |

Observations to carry forward:

- Chrome style reads `HKCU\Software\Google\Chrome\Extensions` (external extension registry) at
  startup — must be disabled in 2.1 (no Google integration).
- A "GPU process exited unexpectedly" error appeared once while idle; watch it in 1.6.
