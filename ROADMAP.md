# Roadmap — Kendime-Brow

All work is tracked here. Design details: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
(numbers in parentheses refer to sections of that document).

**Rule:** As soon as a task is finished (however small), update its status here, add an entry to
[PROGRESS.md](PROGRESS.md) → "Task log", and commit.

**Statuses:** `[ ]` pending · `[~]` in progress · `[x]` done · `[!]` blocked / awaiting decision · `[-]` cancelled

**Current phase:** Phase 1 done except the blocked Ubuntu/codec items (1.1b, 1.2, 1.10b, 1.15). Phase 2 in progress.

**Platform rule (architecture §3.8):** Cross-platform, no primary platform. Every task is verified first
on Windows, then in an Ubuntu VM; no phase counts as "done" until it also works on Ubuntu. macOS comes last (Phase 7).

---

## Phase 0 — Setup and architecture design (no code)

- [x] 0.1 Working folder, git, public GitHub repo
- [x] 0.2 LICENSE (Apache 2.0), .gitignore, README
- [x] 0.3 Roadmap and progress notes skeleton
- [x] 0.4 Technology foundation decision → CEF + Rust
- [x] 0.5 `docs/ARCHITECTURE.md` — full architecture design document
- [x] 0.6 Extra feature selection (architecture §12) → P1 list in scope; P2 deferred, P3 out of scope
- [x] 0.7 Detailing Phase 1+ tasks (this file)
- [x] 0.8 Architecture approval + confirmations → cross-platform (Windows → Ubuntu → macOS); LICENSE name correct

## Phase 1 — Basic browser shell

Output: a navigable, tabbed window with its own UI. Version `0.1`.

- [x] 1.1 **Validation (spike):** V1, V3, V4 — download CEF binary, open a single window with the Rust `cef` crate, test codec status (§2, §14) → V1, V3, V4 confirmed (`spikes/cef-spike`)
- [!] 1.1b **Validation:** V13 — repeat the same experiment in the Ubuntu VM → blocked: needs SSH access to the Ubuntu VM (user)
- [!] 1.2 **Validation:** V2 — try our own CEF build with codecs; record a decision based on the result → blocked: Chromium checkout needs ~100+ GB, only ~60 GB free on D:
- [x] 1.3 Workspace skeleton: `crates/` (including the `kb-platform` crate), `ui/`, `proto/`, `scripts/`, `justfile`, `cef.lock` (§13)
- [x] 1.4 CI (Windows + Ubuntu matrix): fmt, clippy, test, `cargo-deny` license check, `gitleaks` (§13)
- [x] 1.5 `kb://` custom scheme + embedded assets + CSP (§3.4)
- [x] 1.6 Window layout (CEF Views), create/close/switch tabs, navigation (§2.4) → all Alloy style (CEF limits, see §2.4); shortcuts need a manual check
- [x] 1.7 `kb://ui`: tab strip, address bar, back/forward/reload (§3.4)
- [x] 1.8 UI ↔ shell bridge (message router, open only to the `kb://` origin) (§3.3)
- [x] 1.9 `kb-ipc`: named pipe (Windows) + Unix socket (Linux) transport, protobuf, token-based handshake (§3.3)
- [x] 1.10 Module host launcher (Windows): AppContainer/restricted token + Job Object, restart on crash (§3.2) → restricted token + low IL + Job Object done; AppContainer is a follow-up
- [ ] 1.10b **Validation:** V14 → module host sandbox (Linux): user namespace + seccomp + Landlock (§3.8)
- [x] 1.11 Action registry + command palette (§3.5)
- [x] 1.12 Profile, settings (`kb://settings`), `kb.sqlite`: bookmarks, history, session restore (§3.7)
- [x] 1.13 Download manager (basic) (§8.3)
- [x] 1.14 Packaging: Windows installer + Linux `.deb`/AppImage, single taskbar icon, `just install-dev` (§13) → zip + per-user installer (MSI later), .deb + AppImage via CI
- [ ] 1.15 Phase close: verify all Phase 1 features in the Ubuntu VM

## Phase 2 — Privacy and performance core

Output: a daily-usable, ad-free browser lighter than Chrome. Version `0.2`.

- [x] 2.1 Flag/preference set disabling Google services + "zero unwanted connections" CI test (§4.1)
- [x] 2.2 `kb-privacy`: adblock-rust integration, request blocking (§4.2)
- [~] 2.3 Filter list management + updates, cosmetic filtering, scriptlets (§4.2) → lists + updates + cosmetic CSS done; scriptlets pending (needs uBO resources)
- [x] 2.4 Tracking parameter stripping, per-site shield button (§4.2)
- [x] 2.5 Containers (separate `CefRequestContext`), private window, third-party cookie blocking (§4.3) → private window implemented as private tabs (temporary container)
- [x] 2.6 **Validation:** V6 → fingerprinting protection injection, Standard/Strict levels (§4.4)
- [!] 2.7 HTTPS-Only, DoH, WebRTC policy, disable preloading, per-container proxy (§4.5) → code done, BLOCKED: `set_preference` returns 0 without error for every pref (see PROGRESS)
- [x] 2.8 **Validation:** V7 → tab lifecycle: freezing, discarding, sleeping, exceptions (§5.2) → discard via about:blank swap; freeze step skipped
- [ ] 2.9 Memory pressure response, battery/metered connection modes (§5.2, §5.4)
- [ ] 2.10 Resource monitor + `kb://tasks` (§5.5)
- [ ] 2.11 Performance measurement scripts, budgets tracked in CI (§5.1)
- [ ] 2.12 Auto-updater (signed) (§13)

## Phase 3 — File processing modules

Each subgroup can be released independently. Versions `0.3`–`0.5`.

**3A — PDF (§6)**
- [ ] 3A.1 `kb-doc` host + PDFium rendering, `kb://pdf` viewer (tiling, zoom, search)
- [ ] 3A.2 Merge / split / reorder / rotate (lopdf)
- [ ] 3A.3 Form filling, annotations
- [ ] 3A.4 qpdf: encryption, password removal, linearization, repair
- [ ] 3A.5 Compression: image resampling (DPI), color profile (lcms2)
- [ ] 3A.6 OCR pipeline (Tesseract) + language pack download
- [ ] 3A.7 Text and table extraction
- [ ] 3A.8 Signing (PAdES-B-B) + signature verification
- [ ] 3A.9 Batch processing + all PDF actions in the action registry

**3B — Office (§7)**
- [ ] 3B.1 Fast spreadsheet preview with `calamine` (7a)
- [ ] 3B.2 Component downloader (LibreOffice), `kb-doc-lok` subprocess
- [ ] 3B.3 LOK tile rendering + `kb://office` viewer (7b)
- [ ] 3B.4 Format conversion + batch conversion (7b)
- [ ] 3B.5 In-place editing: input events, UNO-command toolbar (7c)
- [ ] 3B.6 Style/template manager, formula bar, macro policy (7d)

**3C — Media and files (§8)**
- [ ] 3C.1 `kb-media` host + FFmpeg component, info panel via `ffprobe`
- [ ] 3C.2 Video: trimming, conversion, presets, raw argument editor, queue
- [ ] 3C.3 Photo: viewer, non-destructive editing, format conversion
- [ ] 3C.4 **Validation:** V11 → metadata/EXIF viewing and selective removal
- [ ] 3C.5 RAW, AVIF, JXL, HEIC support; color management
- [ ] 3C.6 `kb://files`: dual-pane explorer, drag and drop, archives (libarchive)
- [ ] 3C.7 Downloads: automatic checksum, "open with" actions, rules

## Phase 4 — Remote connections and network analysis

Versions `0.6`–`0.7`.

**4A — Vault and remote connections (§10)**
- [ ] 4A.1 `kb-vault`: vault format, keyring, master password, auto-lock (§10.2)
- [ ] 4A.2 Terminal infrastructure: xterm.js, `kb://term`, local shell (ConPTY)
- [ ] 4A.3 SSH (russh): password + key (vault signing), `known_hosts`
- [ ] 4A.4 Connection profiles + import (`ssh config`, PuTTY, `.rdp`) (§10.3)
- [ ] 4A.5 SFTP/SCP → `kb://files` remote pane
- [ ] 4A.6 Tunneling (local/remote/dynamic) + container proxy binding (§10.4)
- [ ] 4A.7 Telnet, FTP/FTPS
- [ ] 4A.8 **Validation:** V8 → RDP (IronRDP), native child window rendering, clipboard
- [ ] 4A.9 Multi-session: split panes, groups, broadcast, encrypted session log (§10.5)

**4B — Network analysis (§9)**
- [ ] 4B.1 `kb-net` + CDP Network observer, `kb://net` request list and details
- [ ] 4B.2 NetLog integration (DNS/TLS/socket events)
- [ ] 4B.3 Filter language (DSL) + field registry
- [ ] 4B.4 HAR/cURL export, edit-and-resend request
- [ ] 4B.5 `kb-capture`: Npcap/pktmon capture, browser-specific filtering
- [ ] 4B.6 TLS key log + pcapng (DSB) writing
- [ ] 4B.7 Protocol dissectors, stream reassembly, three-pane view, statistics

## Phase 5 — VPN/Mesh, Companion, Headscale

Version `0.8`. (§11)

- [ ] 5.1 `docs/SERVER-SETUP.md`: Headscale installation + hardening + backup/restore steps; `deploy/headscale/*.example`
- [ ] 5.2 Server setup (together with the user; credentials never enter the repo)
- [ ] 5.3 **Validation:** V10 → `kb-mesh` (Go/tsnet): Up/Down/Status/Dial, state in the vault
- [ ] 5.4 `kb://mesh`: setup wizard, device list, path type/latency
- [ ] 5.5 `kb-remote` network path = mesh; SOCKS5 → container proxy
- [ ] 5.6 **Validation:** V9 → `kb-companion`: tsnet node, service proxy, Windows service
- [ ] 5.7 Companion readiness checker + status API
- [ ] 5.8 Companion installer + enrollment flow (pre-auth key)
- [ ] 5.9 One-click connect (RDP/SSH/SFTP) end to end
- [ ] 5.10 ACL template, Headscale admin panel (optional)

## Phase 6 — Extra power-user tools

Scope: P1 list (decision 0.6). Version `0.9`.

The other P1 items already exist in earlier phases: command palette → 1.11, resource monitor → 2.10,
local terminal → 4A.2, archives → 3C.6, checksum → 3C.7.

- [ ] 6.1 Password manager: login entries on top of `kb-vault`, password generator, `kb://vault` UI
- [ ] 6.2 Password manager: secure autofill (origin matching, only on user action)
- [ ] 6.3 Password manager: KeePass (KDBX) and CSV import/export
- [ ] 6.4 `kb://tools`: regex tester, JSON format/query, hash, base64/URL/JWT encoding; all in the action registry
- [ ] 6.5 User scripts: manager, `@match`, `GM_*` subset, editor (CodeMirror)
- [ ] 6.6 **Validation:** V5 → Chrome extension (MV3) loading, best effort

Note: every task in Phases 2–6 implements its platform-specific part on both platforms via `kb-platform`
(keyring, PTY, memory pressure, packet capture, service installation — table: architecture §3.8).
The last step of every phase is Ubuntu verification.

## Phase 7 — macOS (when hardware is available)

- [ ] 7.1 **Validation:** CEF app bundle + helper processes on macOS, single window
- [ ] 7.2 `kb-platform` macOS implementations: sandbox, Keychain, Touch ID, PTY, memory pressure
- [ ] 7.3 Companion: launchd daemon, SSH/Screen Sharing readiness check; V15 decision (VNC)
- [ ] 7.4 Signing, notarization, `.dmg`; macOS in the CI matrix
- [ ] 7.5 Verify all modules on macOS

## Deferred (P2 — to be re-evaluated after Phase 6)

REST/API client · encrypted sync · port scanner/ping/traceroute · general OCR ·
batch renaming · screen recording + GIF

## Out of scope (P3)

Git integration · audio editor (conversion/trimming already comes with 3C.2)
