# Progress Notes — Kendime-Brow

Decision log and hand-off notes between sessions. Newest entry on top.
In a new session, read this file and [ROADMAP.md](ROADMAP.md) first.

## Current status (summary)

- **Phase:** 1 in progress. 1.1 done (V1/V3/V4 confirmed). 1.1b and 1.2 blocked (see task log).
- **Next step:** 1.3 workspace skeleton, then 1.4 CI, then 1.5+ in order.
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
