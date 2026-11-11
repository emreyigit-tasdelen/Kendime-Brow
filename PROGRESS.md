# Progress Notes — Kendime-Brow

Decision log and hand-off notes between sessions. Newest entry on top.
In a new session, read this file and [ROADMAP.md](ROADMAP.md) first.

## Current status (summary)

- **Phase:** 1 in progress. 1.1 done (V1/V3/V4 confirmed). 1.1b and 1.2 blocked (see task log).
- **Next step:** 1.10 module host launcher (Windows: AppContainer/restricted token + Job Object), then 1.11+.
- **Needs a manual check by the user:** keyboard shortcuts from 1.6 (Ctrl+T/W/Tab, Alt+←/→, F5).
- **Blocked, needs the user:** 1.1b — SSH access (key) to an Ubuntu VM (`D:/VMware-Virtual-Machines/Ubuntu/...`). 1.2 — ~100+ GB free disk for a Chromium checkout, or a decision to skip proprietary codecs.
- **Build prerequisites (Windows):** VS 2022 Build Tools, CMake and Ninja (winget; add their WinGet package folders to `PATH` if not found).
- **Platform:** Cross-platform, no primary platform. Verification order Windows → Ubuntu (local VM; Kali also available) → macOS (if hardware becomes available).
- **Reading order (new session):** this file → `ROADMAP.md` → only the relevant section of `docs/ARCHITECTURE.md`.

## Standing rules

- **Note-taking rule:** As soon as EVERY task/step in the roadmap is finished (however small), add an entry to the "Task log" section below: which task, what was done, decisions made, next step. No task is skipped; this file is the single source of continuity after `/clear` or running out of tokens. When an entry is added, the ROADMAP status is also updated and a commit is made.

- The repo is public. Keys, tokens, personal data and Headscale credentials never enter the repo. Enforced by the layers in [SECURITY.md](SECURITY.md); after cloning run `git config core.hooksPath .githooks`.
- The commit identity is the GitHub noreply address (no personal email in the public repo).
- Claude never appears as a contributor: no `Co-Authored-By` or "Generated with" attribution lines in commits or PRs.
- License is Apache 2.0. The license compatibility of every added library is noted in the architecture document (GPL/AGPL components are isolated in separate processes).
- **All documentation, the GitHub description and repo metadata are in English.** The only exception is `README.tr.md`, a Turkish translation of `README.md` (the English README is the default). Documents are written clearly enough for a later (cheaper) model to continue.
- Sponsorship: GitHub Sponsors + Buy Me a Coffee (`.github/FUNDING.yml`, README badges and Support section).

## Decisions

| Date | Decision | Rationale |
|---|---|---|
| 2026-11-12 | Repo: `emreyigit-tasdelen/Kendime-Brow`, public, Apache 2.0 | User request |
| 2026-11-12 | VPN/Mesh: WireGuard + self-hosted Headscale | User request; no dependency on a third-party VPN |
| 2026-11-12 | Technology foundation: CEF + Rust | Prebuilt engine binaries, low maintenance burden, modules independent of the engine |
| 2026-11-12 | Architecture decisions K1–K16 | `docs/ARCHITECTURE.md` §1 |
| 2026-11-12 | Cross-platform; order Windows → Ubuntu → macOS | User decision; development environment is Windows + Ubuntu/Kali VM |
| 2026-11-12 | Extra features: P1 in scope, P2 deferred, P3 out of scope | User decision; P1 reuses existing infrastructure |
| 2026-11-12 | All docs in English; Turkish README as a translation; sponsor links | User decision |

## Task log

Newest on top. Format: task · done · decision · next step.

### 1.9 kb-ipc transport — DONE (2026-11-12)
- **Done:** `proto/kb/ipc/v1/ipc.proto` (`Hello`, `HelloAck`, `Envelope`) compiled by `kb-ipc/build.rs` with prost-build + protox (no protoc). `kb-platform::ipc`: endpoint names `kb-<pid>-<module>-<random>`, Windows named pipe (first instance, remote clients rejected), Unix socket in a 0700 runtime dir with 0600 socket file, removed on drop. `kb-ipc`: sync + async length-prefixed framing, `send`/`recv` for protobuf, `Token` (256-bit, hex, constant-time compare, redacted Debug), `accept_handshake` / `client_handshake`. Integration tests over a real named pipe: handshake + envelope round trip, wrong token → PermissionDenied on both sides, second listener on the same name fails. 31 tests pass.
- **Decision / open:** Windows pipe still uses the default DACL (+ token auth). An explicit DACL limited to the current user SID is a follow-up hardening item (do it with 1.10, which needs `windows-sys` security APIs anyway). Linux uses `SOCK_STREAM` + length prefix (not `SEQPACKET`) — same framing on every platform.
- **Next step:** 1.10.

### 1.7 `kb://ui` tab strip + address bar — DONE (2026-11-12)
- **Done:** `ui/` Vite + SolidJS + TypeScript project (solid-js 1.9.17, vite 8.3.5, vite-plugin-solid 2.11.14, typescript 7.0.2 — exact pins, each ≥ 2 weeks old). `strip/` → `kb://ui/` (tab row with spinner, close button, middle-click close, "+"; toolbar back/forward/reload-stop, address bar), `newtab/` → `kb://newtab/` (search box), `src/bridge.ts` typed client. `ui/dist` embedded with `rust-embed` (debug builds read from disk); `scheme.rs` maps hosts to dist folders, shared bundles under `assets/`. CI builds the UI before cargo; `just ui`. Strip height 76 px. Verified with real mouse/keyboard input: "+" opens a tab, clicking a tab switches, typing in the address bar + Enter navigates; screenshots look right (dark theme).
- **Decision / gotcha:** CEF Views `BoxLayout` did not honour `set_flex_for_view` (strip always got the full height, nested Panels stayed 0×0). Replaced with a manual layout: `WindowDelegate::on_layout_changed` → `shell::apply_layout` sets the strip and tab bounds. Vite module-preload polyfill disabled (inline script vs. CSP).
- **Next step:** 1.9.

### 1.8 UI ↔ shell bridge — DONE (2026-11-12)
- **Done:** `protocol.rs` (JSON commands `state`, `subscribe`, `new-tab`, `close-tab`, `activate-tab`, `navigate`, `back`, `forward`, `reload`, `stop`; `State`/`TabState`; address bar rule `input_to_url`; `is_trusted_origin`; 4 tests) and `bridge.rs` (cef-rs message router: `window.kbQuery` / `kbQueryCancel`). Renderer side injects the functions ONLY into `kb://` frames; browser side re-checks the frame URL (403 otherwise). `subscribe` is a persistent query that receives the full state on every change. Shell keeps per-tab title/url/loading/back/forward (load + display handlers) and broadcasts on change. Verified with `scripts/bridge-smoke.mjs`: web page has no `kbQuery`, `kb://ui` does; commands work; bad command → 400.
- **Decision / gotcha:** running a command inside `on_query_str` deadlocks when it creates a browser (the router is re-entered via `on_before_browse`). Commands are answered first and executed in a posted UI task. Default search engine for non-URL input: DuckDuckGo (setting in 1.12).
- **Next step:** 1.7 (the UI that uses this bridge).

### 1.6 Window layout and tabs — DONE (2026-11-12), shortcuts need a manual check
- **Done:** Replaced the cefsimple code with `app.rs` (App, BrowserProcessHandler, Client, Views delegates), `shell.rs` (UI-thread state: window, content panel, tabs; new/close/cycle tab, back/forward/reload, title, do_close/on_before_close, window close) and `tabs.rs` (pure tab list: insert-after-active, close → right then left, cycle; 5 tests). Layout: vertical box, `kb://ui` strip (44 px) + content panel with one BrowserView per tab, only the active one visible. Popups (`window.open`) open as new tabs. Verified via DevTools: popup → new tab, closing that tab keeps the window and updates the title, closing the last tab closes the window and the process exits.
- **Decision:** CEF limits found: a Chrome-style BrowserView cannot be added to an Alloy-style window, and a window cannot hold more than one Chrome-style BrowserView → tabbed window is **Alloy style throughout** (ARCHITECTURE §2.4 and V3 updated; Chrome extensions would need a separate Chrome-style window). Closing a tab: `do_close` returns true and the BrowserView is removed, otherwise CEF closes the window.
- **Not verified:** keyboard shortcuts (Ctrl+T/W/Tab, Alt+←/→, F5). Implemented in three places (window accelerators, `KeyboardHandler::on_pre_key_event`, `WindowDelegate::on_key_event`), but synthetic input (SendKeys / keybd_event) only produced CHAR events, never key-down, so they could not be tested automatically. **User: please try them by hand.** Debug output: run with `KB_DEBUG=1`.
- **Next step:** 1.7.

### 1.4 CI matrix — DONE (2026-11-12)
- **Done:** `.github/workflows/ci.yml`: Windows + Ubuntu matrix (fmt, clippy `-D warnings`, test; CMake/Ninja/GTK deps on Linux), `cargo-deny` job (licenses/bans/sources, `deny.toml`). Fixed a `.gitignore` bug (`bin/` ignored `src/bin`). Both platforms green.
- **Decision:** The Ubuntu CI build compiles the CEF shell on Linux — partial V13 evidence (compile only; running still needs the VM, 1.1b).
- **Next step:** 1.5.

### 1.5 `kb://` scheme + embedded assets + CSP — DONE (2026-11-12)
- **Done:** `kb-shell/src/scheme.rs` (pure URL → asset resolution, traversal-safe, MIME table, CSP constant; 4 tests) and `kb_scheme.rs` (CEF glue: `kb` registered as standard + secure + CORS + fetch in every process via `App::on_register_custom_schemes`; helper binary now passes `KbSubprocessApp`; handler factory installed in `on_context_initialized`). Placeholder pages `kb://ui/` and `kb://newtab/` (new default start page). Responses carry `Content-Security-Policy`, `X-Content-Type-Options: nosniff`, `Cache-Control: no-store`. Verified in the real window: headers present (`scripts/cdp-headers.mjs`), inline script blocked (the same page via `file://` runs it).
- **Decision / gotcha:** `cef` wrapper macros CLONE struct fields; a cloned `CefStringMultimap` is only a borrowed pointer to the original, so the stock `StreamResourceHandler` loses its header map. Own `KbResourceHandler` writes headers in `response_headers` and keeps shared state in `Arc`. Remember this for every `wrap_*!` struct. Assets use `include_bytes!` until the Vite build exists (1.7 → `rust-embed`).
- **Next step:** 1.6.

### 1.3 Workspace skeleton — DONE (2026-11-12)
- **Done:** Root `Cargo.toml` workspace (resolver 3, edition 2024, `cef = "=156.4.0"` pinned), `cef.lock` (CEF 156.0.26, Chromium 156.0.8078.88, archive SHA-1). Crates: `kb-shell` (binary `kendime-brow` + `kb-shell-helper`, bootstrapped from the spike, default page `about:blank`), `kb-platform` (platform detection + data directory per §3.7/§3.8, tested), `kb-ipc` (4-byte length-prefixed framing, tested), `kb-actions` (action registry, search, chain type-checking, tested). `ui/`, `proto/`, `scripts/` placeholders, `justfile` (build/dev/test/check/scan). 11 tests pass; fmt + clippy `-D warnings` clean; the shell window opens.
- **Decision:** Crates are added only when their task starts (no empty `kb-vault`, `kb-doc` … yet). macOS code from cefsimple stays in `kb-shell/src/shared` until 1.6 replaces it.
- **Next step:** 1.4.

### 1.1 CEF + Rust spike — DONE (2026-11-12)
- **Done:** `spikes/cef-spike`: cefsimple port on `cef = 156.4.0` (Chromium 156), builds with MSVC + CMake + Ninja (installed via winget). A single window opens in Chrome style and in Alloy style. Results in `spikes/cef-spike/README.md`.
- **Decision:** V1 confirmed (no H.264/AAC/HEVC in official CEF; VP9/AV1/Opus/MP3 OK). V3 confirmed per style. V4 confirmed: the crate covers Views, request handler, DevTools messages, schemes, request contexts, render process handler and ships a Rust message router — no C++ bridge needed.
- **Blocked:** 1.1b (no SSH access to the Ubuntu VM; VMs are powered off and need guest credentials). 1.2 (a Chromium source build needs ~100+ GB; ~60 GB free on D:). Continue without proprietary codecs until decided.
- **Next step:** 1.3.

### Sensitive data protection (2026-11-12)
- **Done:** Added `SECURITY.md` (policy + 7 protection layers + leak response). Local hooks in `.githooks/` (gitleaks `pre-commit` with `.gitleaks.toml` incl. personal-email rule; `commit-msg` blocking AI attribution and non-noreply emails), tested with fake data. Enabled GitHub secret scanning + push protection. Added CI workflow `.github/workflows/secret-scan.yml` (full-history gitleaks + author check). User enabled the GitHub email privacy push block.
- **Decision:** Layered defense; on a leak, revoke first, then rewrite history.
- **Next step:** Phase 1.1.

### Docs in English + sponsor links (2026-11-12)
- **Done:** `README.md` rewritten in English (default) with GitHub Sponsors / Buy Me a Coffee badges and a Support section; `README.tr.md` added as the Turkish translation. `ROADMAP.md`, this file and the architecture document translated to English. Renamed `ilerleme-notlari.md` → `PROGRESS.md`, `docs/mimari.md` → `docs/ARCHITECTURE.md`. Added `.github/FUNDING.yml`. GitHub repo description set in English.
- **Decision:** English is the project language for all documents; Turkish only in `README.tr.md`.
- **Next step:** Phase 1.1.

### 0.8 Architecture approval and confirmations — DONE (2026-11-12)
- **Done:** User confirmations received. Added §3.8 "Cross-platform support" to the architecture (platform abstraction table, `kb-platform` crate, rules); updated K15, §3.2, §13, §14 (V13–V15, two new risks). Roadmap: CI Windows + Ubuntu matrix, Ubuntu tasks 1.1b / 1.10b / 1.15, new Phase 7 (macOS); removed the old 6.7 "Linux support" because Linux is now part of every phase.
- **Decision:** The product is **cross-platform**, with no primary platform. Verification order: Windows → Ubuntu → macOS (last, if hardware becomes available). Platform-specific code lives only in `kb-platform`. No phase counts as done until it also works on Ubuntu. The name "Emre Yiğit Taşdelen" in LICENSE is correct.
- **Next step:** Phase 1.1.

### 0.6 Extra feature selection — DONE (2026-11-12)
- **Done:** The user approved the prioritization in architecture §12. `ROADMAP.md` Phase 6 was split into tasks according to P1 (6.1–6.7); a decision note was added to architecture §12.
- **Decision:** **P1 in scope** (command palette, resource monitor, local terminal, password manager, checksum, archives, regex/JSON/hash tools, user scripts). **P2 deferred** (REST client, sync, port scanner, general OCR, batch renaming, screen recording). **P3 out of scope** (Git integration, audio editor).
- **Next step:** 0.8 confirmations (non-blocking) → Phase 1.1: download the CEF binary, open a single window with the Rust `cef` crate, validate V1/V3/V4.

### 0.7 Detailing Phase 1+ tasks — DONE (2026-11-12)
- **Done:** `ROADMAP.md` rewritten: Phases 1–6, ~70 tasks in total, each referencing an architecture section. Validation (spike) tasks mapped to V1–V12.
- **Decision:** Every phase produces an independently releasable version (0.1 … 0.8). Phase 3 split into three independent groups (PDF / office / media).
- **Next step:** user decisions 0.6 and 0.8.

### 0.5 Architecture design document — DONE (2026-11-12)
- **Done:** `docs/ARCHITECTURE.md` written (14 sections): process model, sandbox, IPC, privacy, performance, PDF/office/media, network analysis, remote connections, mesh + Companion + Headscale server recommendations, extra feature prioritization, repo layout, assumptions to validate.
- **Decisions (K1–K16, details in §1 of the document):**
  - UI in HTML/TS (SolidJS), `kb://` scheme. Heavy modules run in separate processes, started on demand.
  - Processes that parse untrusted files (kb-doc, kb-media) have no network access.
  - PDF: PDFium + qpdf + lopdf + Tesseract. Office: LibreOfficeKit (optional download). Media: FFmpeg LGPL build, subprocess.
  - SSH: russh. RDP: IronRDP (fallback FreeRDP). Vault: separate process, private keys never leave the vault.
  - Mesh: `tsnet` (Go) — needs no driver/admin rights, not a system-wide VPN. Companion: Go, embedded tsnet, proxies RDP/SSH from loopback.
  - GPL/AGPL components (Ghostscript, MuPDF, x264, exiv2, OnlyOffice) are not linked into the code.
- **Key finding:** Official CEF binaries may have H.264/AAC disabled (V1). If so, we need our own unpatched CEF build with codec arguments (V2). The first two tasks of Phase 1 validate this.
- **Next step:** 0.7.

### 0.4 Technology foundation decision — DONE (2026-11-12)
- **Done:** 5 options compared (table below in the "Journal" section) and presented to the user.
- **Decision:** **C — CEF + Rust native shell** (user approved). A migration path to an ungoogled-chromium-patched CEF build remains open if needed (architecture §2.3).
- **Next step:** 0.5.

### 0.3 Roadmap and progress notes skeleton — DONE (2026-11-12)
- **Done:** Created `ROADMAP.md` (Phase 0 detailed, Phases 1–6 placeholders) and the progress notes file (now `PROGRESS.md`).
- **Decision:** Status markers `[ ] [~] [x] [!] [-]`. Phase 1+ tasks to be filled in once the architecture document is done.
- **Next step:** 0.4.

### 0.2 LICENSE, .gitignore, README — DONE (2026-11-12)
- **Done:** Apache 2.0 text taken from apache.org, copyright line set to "2026 Emre Yiğit Taşdelen". `.gitignore`: keys, .env, WireGuard/Headscale files, pcap/har, build outputs, node_modules, Chromium source tree, browser profiles. `README.md` written.
- **Decision:** The name in LICENSE was taken from the Windows user name (confirmed by the user in 0.8).
- **Next step:** 0.3.

### 0.1 Folder, git, GitHub repo — DONE (2026-11-12)
- **Done:** `git init` in `D:\Claude\Kendime-Brow` (branch: main). Public repo `emreyigit-tasdelen/Kendime-Brow` created with `gh repo create`, first commit pushed. Pre-commit sensitive data scan clean.
- **Decision:** Commit identity configured only in this repo, GitHub noreply address (no global git identity was set; keeps the personal email out of the public repo).
- **Next step:** 0.2.

## Journal

### 2026-11-12
- Created the folder, git, GitHub repo, LICENSE, .gitignore, README, ROADMAP and this file.
- Compared options for the technology foundation and asked the user (below).

#### Technology foundation options

| | A. Chromium fork | B. Electron | C. CEF + native shell | D. Firefox/Gecko fork | E. Tauri/system WebView |
|---|---|---|---|---|---|
| Web compatibility | Full | Full | Full | Good | Varies by platform |
| Privacy control | Highest (source-level patches) | Low (limited access to the network stack) | Medium-high | High | Low |
| RAM/CPU control | Highest | Low | High | High | Medium |
| Chrome extension support | Full (MV2 can be kept) | Partial | Partial | WebExtensions | None |
| Build cost | Very high (100+ GB, hours) | Low | Low (prebuilt binaries) | High | Low |
| Security update burden | High (constant rebasing) | Medium | Medium | High | Low |
| Suitability for a solo developer | Hard | Easy but weak for a browser | Suitable | Hard | Not suitable for a browser |

**Chosen:** C (CEF + Rust native shell).
