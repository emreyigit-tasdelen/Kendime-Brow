# Progress Notes — Kendime-Brow

Decision log and hand-off notes between sessions. Newest entry on top.
In a new session, read this file and [ROADMAP.md](ROADMAP.md) first.

## Current status (summary)

- **Scratch disk (user-granted):** `C:\Claude-Temp` is mine for temporary/large files (build output, downloads, VMs/Chromium source). Hard budget: stay under **100 GB total**. Plan: set `CARGO_TARGET_DIR=C:\Claude-Temp	arget` for builds when `D:` runs low, keep downloads in `C:\Claude-Temp\dl`, check size only occasionally with `du -sh /c/Claude-Temp` (don't poll). Never put secrets there; delete when done.
- **Where we stopped (2026-11-13):** Phase 1 done except blocked items; Phase 2 mostly done; Phase 3A.1/3A.2 done, 3A.3 (forms) and 3A.7 (text) partial, 3B.1 (sheet preview) and 3C.4 (EXIF) done, 6.4 (`kb://tools`) done, 4A.1 (vault core) partial. **In progress: 4A.2 terminal** — `terminal.rs` (portable-pty / ConPTY, tested), bridge (`term-open` persistent query, `term-write/resize/close`), `kb://term` page with xterm.js 6.0.0 + fit addon are written and compile; live check DONE via `scripts/term-smoke.mjs` + `scripts/term-close.mjs` (typing, output, size, tab close kills the shell). Next: 4A.1b/c vault host + `kb://vault` UI.
- **4A.1b DONE (2026-11-13):** `vault.rs` service in the shell (single `vault` bridge command: status/create/unlock/lock/list/get/put/remove, 5-min idle auto-lock), `kb://vault` UI (create/unlock/list/edit/generate/copy+clear clipboard/delete), action `vault.open`, verified live with `scripts/vault-smoke.mjs`. Still open: run vault in a sandboxed host (4A.1c), OS keyring / Windows Hello unlock, password autofill (6.1).
- **4A.3 core DONE:** new crate `kb-ssh` (russh 0.65, `ring` backend because aws-lc needs NASM): password/key auth, own `known_hosts` (TOFU, changed key always refused), shell + resize, 4 tests against an in-process server. Still open: wire into `kb://term` (connect form, vault keys), jump hosts, agent forwarding.
- **4A.3 wired (2026-11-13):** `kb://term/?host=&port=&user=&vault=` opens an SSH session (password prompt or vault entry; ssh-key entries keep PEM in `password`, passphrase in `notes`), unknown-host fingerprint prompt then TOFU into `<profile>/known_hosts`, changed key refused. Verified live with `scripts/ssh-smoke.mjs` + `cargo run -p kb-ssh --example testserver`. Observed once: the very first reconnect right after a refused host key got a connection reset (not reproducible; retry worked). Open: connection-profile UI (4A.4), jump hosts, agent.
- **4A.4 DONE (2026-11-13):** `connections.rs` (JSON profiles in the profile dir, ~/.ssh/config import with a unit test), `kb://connections` UI (list/edit/delete/connect, vault entry picker), action `connections.open`; bridge op verified live. Open: PuTTY and `.rdp` import, groups/folders. Next: 4A.5 SFTP.
- **4A.5 DONE (2026-11-13):** `kb_ssh::sftp` (list/download/upload/mkdir/remove/rename; whole-file in memory, 512 MB cap) tested against an in-memory SFTP server in the unit tests; `sftp_svc.rs` + `kb://files` page (browse, upload via file picker up to 64 MB, download to ~/Downloads without overwriting, host key prompt); wiring verified live (error path only, no real SFTP server available). Open: streamed/chunked transfers, SCP, drag and drop, progress. Next: 4A.6 tunnels.
- **NEEDS USER (CI):** the `Secret scan` workflow fails on every push since commit `862c928` ("Relicense", made by the user) because its author email is a real work address, not the GitHub noreply one. Fixing means rewriting that commit's author and force-pushing (history rewrite = user's call), or allowlisting that SHA in `.github/workflows/secret-scan.yml`. Not touched.
- **4A.6 DONE (2026-11-13):** `kb_ssh::tunnel` (local forward + SOCKS5 CONNECT, 127.0.0.1 only; 3 tests incl. SOCKS by address and by name), `tunnels.rs` service and a Tunnel panel + active-tunnel list on `kb://connections`; verified live through the bridge (the example test server also echoes channel data, so HTTP replies come back prefixed with the request: a quirk of the example, not of the tunnel). Open: remote forwards (-R), binding a container's proxy to a tunnel (blocked by the `set_preference` bug, see 2.7). Next: 4A.7 Telnet/FTP, then Phase 3 leftovers.
- **Next steps, in order:**
  1. Finish 4A.2: run the app, open `kb://term/`, verify typing / output / resize / session kill on tab close; commit.
  2. 4A.1b/c: run `kb-vault` as a sandboxed host, `kb://vault` UI (also 6.1 password manager), OS keyring/Windows Hello unlock, auto-lock.
  3. 4A.3 SSH (russh), 4A.4 profiles, 4A.5 SFTP, 4A.6 tunnels.
  4. Phase 3 leftovers: 3A.3 annotations/radio/combo, 3A.4 qpdf ops, 3A.5 compression, 3A.6 OCR, 3A.8 signing, 3A.9 batch + registry actions, 3B.2+ LibreOffice, 3C.1–3C.3/3C.5–3C.7 (FFmpeg, photo editing, files).
  5. Phase 2 leftovers: 2.3 scriptlets (needs uBO resources), 2.7 preferences bug (`set_preference` returns 0), 2.11 RAM (454 MB vs 250 MB budget), 2.12 needs the maintainer's minisign public key.
  6. Phase 4B network analysis, Phase 5 (Headscale/mesh/companion), rest of Phase 6, Phase 7 (macOS).
- **Blocked / needs the user:** Ubuntu VM SSH access (1.1b, 1.10b, 1.15); ~100 GB disk for 1.2; minisign key (2.12); manual check of keyboard shortcuts from 1.6 / 1.11; CI runs for the latest commits were still in progress (check `gh run list`).
- **Build prerequisites (Windows):** VS 2022 Build Tools, CMake and Ninja (winget; add their WinGet package folders to `PATH`), Node 24, PDFium via `scripts/fetch-pdfium.ps1` (`PDFIUM_PATH` for kb-doc tests).
- **Test habit:** no foreground/mouse/keyboard tests while the user is at the computer — DevTools-only checks (`scripts/*.mjs`, `q.mjs`-style bridge calls).

## Standing rules

- **Note-taking rule:** As soon as EVERY task/step in the roadmap is finished (however small), add an entry to the "Task log" section below: which task, what was done, decisions made, next step. No task is skipped; this file is the single source of continuity after `/clear` or running out of tokens. When an entry is added, the ROADMAP status is also updated and a commit is made.

- The repo is public. Keys, tokens, personal data and Headscale credentials never enter the repo. Enforced by the layers in [SECURITY.md](SECURITY.md); after cloning run `git config core.hooksPath .githooks`.
- The commit identity is the GitHub noreply address (no personal email in the public repo).
- Claude never appears as a contributor: no `Co-Authored-By` or "Generated with" attribution lines in commits or PRs.
- License is AGPL-3.0. The license compatibility of every added library is noted in the architecture document (GPL/AGPL components are isolated in separate processes).
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

### 3A.3 PDF forms — PARTIAL (2026-11-12)
- **Done:** `kb-doc::forms` (`list_fields`: page, name, kind, value, checked; `fill`: text fields and checkboxes by field name; unknown names are an error), host methods `pdf.fields` / `pdf.fill`, bridge `pdf-fields` / `pdf-fill` (writes `<name> (filled).pdf`), viewer "Fill form" panel. Tested with a hand-built AcroForm PDF and real PDFium (also live through the sandboxed host: fields listed, filled copy written, typo rejected).
- **Pending:** radio / combo / list fields, in-page form rendering (the rendered page does not show typed values until saved and re-opened), annotations (highlight, notes, drawing), flattening.
- **Next step:** 3A.9 / annotations, then 3A.4 qpdf operations.

### 3A.7 Text extraction + search — PARTIAL (2026-11-12)
- **Done:** `kb-doc::extract_text` (per-page text via PDFium) and `search_pages` (case-insensitive, snippet around the first hit, char-boundary safe); host method `pdf.text`; bridge `pdf-text`; viewer: search box (Enter / Shift+Enter cycle through matching pages, "n/m pages"), "Copy all text" button. Tested with real PDFium (5 pages, hit on page 5, empty/whitespace queries) and live (2-page file returns both texts).
- **Pending:** table extraction (CSV/XLSX), text selection / in-page highlight, OCR (3A.6).
- **Next step:** 3A.3 forms + annotations (or 3A.9 batch / registry actions).

### 3A.2 Merge / split / reorder / rotate — DONE (2026-11-12)
- **Done:** `kb-doc::edit`: `merge`, `extract` (split / extract / reorder / duplicate in one primitive), `rotate` (relative, multiples of 90°, all or selected pages), `parse_page_ranges` (`1-3,5,8-`, 1-based, validated). Built on PDFium page import (not lopdf) so annotations, forms and fonts are kept. Tests: unit test for ranges + integration test with real PDFium (merge 3+2 pages, text order checked, split/reorder `[4,1,1]`, rotation size swap and round trip, error cases).
- **Done (second part):** host methods `pdf.merge` (packed documents), `pdf.extract`, `pdf.rotate` (page ranges validated in the host); shell `pdf.rs` (`edit_file`, `merge_files`, unique `<name> (merged|pages|rotated).pdf` next to the input), bridge `pdf-extract` / `pdf-rotate` / `pdf-merge` answered from worker threads, viewer tool bar (rotate all ↻/↺, extract pages, merge by typed paths, "Open result"). Verified live: rotate, extract page 2, merge of two files each wrote a file; page 9 of 2 → clear error.
- **Pending UI:** thumbnails with drag reorder, file picker for merge (typed paths for now), registry actions `pdf.*`, 16 MB total limit.
- **Next step:** 3A.3 form filling + annotations.

### 3A.1 PDF host + viewer — DONE (2026-11-12)
- **Done (second part):** `kb-doc` host (`kb-doc-host`) with `service.rs` (`pdf.open` bytes → `{pages,width,height}`, `pdf.render` `{page,dpi}` → PNG, `pdf.close`), tested through the real sandbox (restricted token, low IL, Job Object): it loads PDFium and renders. Shell: `pdf.rs` (service thread with its own tokio runtime, lazy host start, restart + re-open after a crash, path validation: absolute `.pdf`, ≤ 16 MB), bridge `pdf-open` / `pdf-render` (base64 PNG, answered from worker threads), `pdf_url.rs` (file:// ↔ viewer URL; 2 tests), `on_before_browse` redirects `file://….pdf` main-frame navigations to `kb://pdf/?file=…`, viewer page `ui/pdf` (lazy rendering with IntersectionObserver, zoom 25–400 %). Packaging includes `kb-doc-host.exe` and `pdfium.dll` (from `scripts/fetch-pdfium.ps1`). Verified live: 2-page PDF rendered with crisp text; one `kb-doc-host` process.
- **Limits / next:** files > 16 MB (IPC frame cap) need handle passing or chunked transfer; http(s) PDFs download instead of opening (open-after-download button pending); no text search/selection yet; CI does not fetch PDFium (kb-doc tests skip there); Linux packaging of `libpdfium.so` pending; `kb-doc-host` currently loads PDFium via `PDFIUM_PATH` or next to the exe.
- **Next step:** 3A.2 merge/split/reorder/rotate (lopdf).

### 3A.1 PDF engine (PDFium) — first part (2026-11-12)
- **Done:** new crate `kb-doc` (`pdfium-render` 0.9.5 MIT/Apache; PDFium 8152 binary from bblanchon/pdfium-binaries, BSD-3, loaded at runtime from `PDFIUM_PATH` / next to the exe). `info` (page count, first page size) and `render_page_png(dpi)`; integration test builds a one-page PDF with correct xref, renders it at 144 dpi → 400×200 PNG with real text pixels, and checks errors (bad page, not a PDF) — passes with the pinned PDFium; skips when the library is absent (CI does not fetch it yet). `scripts/fetch-pdfium.{ps1,sh}` pin tag + SHA-256 (Windows hash verified against the release digest).
- **Pending for 3A.1:** `kb-doc` host process on `kb-ipc` (methods info/render; file access by handle — the sandboxed low-IL host cannot read user files by path), `kb://pdf` viewer (tiles, zoom, search), CI step to fetch PDFium, bundling `pdfium.dll` into packages.

### 2.12 Signed updater — PARTIAL (2026-11-12)
- **Done:** `updater.rs`: version parsing/comparison, GitHub Releases `latest` check (pre-releases ignored), platform asset + mandatory `<asset>.minisig` selection (unsigned releases are never offered), minisign verification with `minisign-verify` (tested with a real vector: good / tampered / missing / bad key). `UPDATE_PUBLIC_KEY` is empty → updater disabled (`Check::Disabled`), nothing contacts GitHub.
- **Needs the maintainer:** generate the release signing key (`minisign -G`), keep the secret key OUT of the repo (CI secret), put the public key into `UPDATE_PUBLIC_KEY`, and sign release assets in the package workflow. After that: download + verify + apply-on-restart step, an update setting/UI notice, and allowlisting `api.github.com` / release hosts in `check-connections.mjs`.
- **Gotcha:** enabling ureq's `json` feature in kb-shell changed ureq's behavior for the CEF download build script (10 MB body limit → CEF build failed); keep `json` off and parse manually.
- **Next step:** Phase 3.

### 2.11 Performance scripts — PARTIAL (2026-11-12)
- **Done:** `scripts/perf.mjs` (cold start until `kb://ui` is loaded; memory sum of the process tree after 10 s; exit 1 when over budget). Release build on this machine: **cold start 681 ms (budget 800 ✔)**; **memory 702 MB over 13 processes with one empty tab (budget 250 ✘)**.
- **Why / next:** Windows working-set numbers double-count shared pages, and the tree includes spare renderers, GPU and network/storage utility processes. Real levers to try: `--renderer-process-limit`, `--disable-gpu`-style / in-process GPU options, `--disable-features` for spare renderer, single network service. Needs measuring with private-bytes instead of working set. CI does not run it (no desktop session); run locally.
- **Update:** `--in-process-gpu` is now a default switch: 671 → 454 MB, 15 → 11 processes (`PERF_ARGS` env var of perf.mjs lets you try more flags). `--renderer-process-limit=1` saves another ~40 MB but would remove per-site isolation for multi-tab use, so it is NOT applied. Still above the 250 MB budget; remaining cost is mostly the browser process itself (462 MB debug / ~300 MB release) and the network/storage utilities.
- **Next step:** 2.12.

### 2.10 Resource monitor — DONE (2026-11-12)
- **Done:** `tasks.rs` (sysinfo 0.40): browser process + descendants with kind (browser / renderer / gpu / utility from `--type=`), memory, CPU; `kill` only for descendants. Bridge `tasks`, `kill-process`; `kb://tasks/` page refreshing every 2 s with "End process"; action `tasks.open`; 2 tests. Verified live (browser, utility, 3 renderers, gpu listed).
- **Not done:** mapping renderers to tabs, per-tab indicator in the strip, freeze/discard buttons.
- **Next step:** 2.11.

### 2.9 Battery / memory pressure — DONE (2026-11-12)
- **Done:** `kb-platform::power` (Windows `GetSystemPowerStatus` + `GlobalMemoryStatusEx`; Linux sysfs + /proc/meminfo; others neutral). The 2.8 discard task now uses a 5 min idle limit on battery and 1 min when < 10 % memory is available. Polling every 30 s replaces `QueryMemoryResourceNotification` events.
- **Not done:** earlier freezing, 30 fps cap, autoplay off and metered-connection handling from §5.4.

### 2.8 Tab lifecycle (V7) — DONE (2026-11-12)
- **Done:** every tab tracks `last_active`; a 30 s UI-thread task discards background tabs idle ≥ 30 min (`KB_DISCARD_SECS` overrides; skips loading tabs, non-http(s) pages, already discarded; pure rule `should_discard` tested). Discard = navigate the tab to `about:blank` (frees the page's renderer memory) while the strip keeps title/URL and the session keeps the real URL; activating the tab reloads the saved URL. Verified: with a 20 s limit the background tab went `about:blank` in DevTools while the UI still showed "Example Domain", and returned to `https://example.org/` on activation.
- **V7:** the CDP freeze step (`Page.setWebLifecycleState`) was not tried; discarding alone is implemented. Exceptions for audio/mic/forms/pinned tabs are not implemented yet (needs audio-state callbacks) — follow-up.
- **Next step:** 2.9.

### 2.7 Network privacy — BLOCKED (2026-11-12)
- **Done (code):** `net_privacy.rs` (DoH providers Quad9/Mullvad/Cloudflare, proxy validation, HTTPS-Only/DoH/proxy appliers, 2 tests), settings (`https_only`, `doh`, `proxy_<container>`), settings-page controls, re-applied per container context.
- **Blocked:** `RequestContext::set_preference` / `PreferenceManager::set_preference` return 0 with an empty error for EVERY pref (even `has=1 can=1`, value type correct, reading back shows the old value). This also means the prefs from 2.1/2.5/2.6 (search suggest, safebrowsing, third-party cookies, WebRTC policy…) are NOT applied — only the command-line switches and per-container isolation work. Suspects: cef-rs binding of the `error` out-param, or prefs only applying at a different moment. HTTPS upgrade seen in tests was HSTS, not our setting.
- **Also tried (no change):** setting through the browser's own `request_context()` after creation, with an `error` out-param: still `r=0`, empty error, value unchanged. `json_parse` is not exposed by the bindings. Next ideas: a C++ shim calling `CefRequestContext::SetPreference`, or a newer cef-rs release.
- **Next:** investigate (try a `CefDictionaryValue`/`SetPreference` call via a tiny C++ shim, or `--enable-features` / policy-style alternatives); until then rely on switches. Tracked as a 2.7 follow-up.

### 2.6 Fingerprinting protection (V6) — DONE (2026-11-12)
- **Done:** `kb-privacy::fingerprint` (levels Off/Standard/Strict, FNV-1a per-(session, site) seed, injected script; 2 tests). Script: canvas `getImageData`/`toDataURL`/`toBlob` and WebGL `readPixels` get deterministic low-bit noise; WebGL unmasked vendor/renderer generic; AudioBuffer/Analyser noise; `hardwareConcurrency` = 8, `deviceMemory` = 8; Battery/Bluetooth/USB/Serial/HID removed; Strict adds rounded screen size, en-US, UTC; overridden functions report `[native code]`. Browser process creates a random session seed and passes `--kb-fp-seed` / `--kb-fp-level` to child processes (`on_before_child_process_launch`); the renderer evals the script in `on_context_created` for every http(s) frame (main world, before page scripts). Setting `fingerprint_protection` (+ settings select). WebRTC `ip_handling_policy = disable_non_proxied_udp` added to the profile prefs.
- **V6 result:** confirmed — injection at context creation works in all frames before page scripts. Measured with a canvas probe: same site → same hash, other site → different, new session → different, all different from the unprotected hash; cores 12→8, memory 32→8, Battery/USB undefined.
- **Note:** the level is read when a renderer process starts; Chromium keeps a spare renderer, so a change is fully effective after restart (UI says so). Font enumeration protection (§4.4) not done yet — needs a font allowlist in the renderer; later.
- **Next step:** 2.7.

### 2.5 Containers, private tabs, third-party cookies — DONE (2026-11-12)
- **Done:** `containers.rs`: Personal (global context), Work (own persistent `CefRequestContext` under `<profile>/cef/containers/work`), Private (`temporary`, in-memory context dropped when its last tab closes). Tabs carry their container; new tabs inherit the active tab's; popups inherit the opener's; `new-tab {container}`; actions `tab.new-private`, `tab.new-work`, `tab.new-personal`. Private tabs are never written to history or the session. Session table got a `container` column (schema v2, in-place migration). Third-party cookies blocked via profile prefs (`profile.cookie_controls_mode = 1`, `profile.block_third_party_cookies`), and all privacy prefs are applied to every container context. UI: orange (Work) / purple + "Private" tag (Private) tab marking. Verified via DevTools `Target.getTargets` (distinct `browserContextId`s): a cookie set in Personal is invisible in Private and Work; a Private cookie is gone after closing all private tabs and opening a new one; restart restores Personal + Work tabs, not Private.
- **Decision:** the "private window" of §4.3 is a private *tab* (same window, purple marking) — avoids a second window/shell instance for now; a separate window can come later.
- **Gotcha:** my first isolation test mapped DevTools pages by `/json` order and wrongly reported shared cookies; identify targets by `browserContextId` instead.
- **Next step:** 2.6.

### 2.4 Tracking parameters + per-site shield — DONE (2026-11-12)
- **Done:** `kb-privacy::url_clean`: `strip_tracking_params` (`utm_*`, `pk_*`, `mtm_*`, `hsa_*`, `fbclid`, `gclid`, `msclkid`, `_ga`, …; fragment kept), `unwrap_redirect` (Google `/url`, Facebook `l.php`, Instagram, Reddit, YouTube `/redirect`, Slack, Steam, Disqus — only http(s) targets), `clean_url`, `host_of`; 4 tests. Main-frame and sub-frame requests are rewritten to the clean URL in `on_before_resource_load` (documents are still never blocked). Shield: `shield_off` table in `kb.sqlite` mirrored to an in-memory set for the IO thread; the main-frame URL decides; `toggle-shield` command + `shield.toggle` action (reloads the tab); `TabState.shield`; 🛡 button with blocked-count badge (greyed when off). Verified via DevTools only: `example.org/?utm_source=…&id=1&fbclid=…` → `example.org/?id=1`; theguardian.com 14 blocked → shield off 0 → on again 11.
- **Next step:** 2.5 (then finish 2.3 scriptlets).

### 2.3 Filter lists, updates, cosmetic filtering — PARTIAL (2026-11-12), scriptlets pending
- **Done:** `kb-privacy::lists`: catalog (EasyList, EasyPrivacy, uBlock filters, uBlock privacy, Peter Lowe — on by default; Turkish list — off), `lists.json` state (enabled, last update, rule count, error), 24 h interval, sanity check against HTML error pages, atomic file replace, `custom*.txt` support, `download` via `ureq` (20 MB cap, 60 s timeout); 5 tests. `Blocker::load_files`, `Blocker::cosmetic(url)` → page-specific hide selectors (minus exceptions) as one stylesheet (+ scriptlet script, empty for now); test. Shell: startup reload + background update thread (engine swapped live, UI refreshed via a posted task); setting `filters_auto_update`; cosmetic CSS injected in `LoadHandler::on_load_start` as a constructable stylesheet (not affected by page CSP). Bridge: `set-filter-list`, `update-filters`; `profile.filters`. Settings page: "Content blocking" section (per-list toggle, rules, last update / error, auto-update, "Update now"). `check-connections.mjs` allowlists the list hosts (read from `lists.rs`). Verified: fresh profile downloads all 5 default lists (~145 k rules); theguardian.com → 13 requests blocked.
- **Pending:** scriptlets (`##+js(...)`) need uBlock Origin's resource bundle loaded with `Engine::use_resources` (GPL, must be downloaded like the lists) — next part of 2.3. Generic cosmetic filters (class/id selectors via `hidden_class_id_selectors`) need a DOM observer in the page — also pending.
- **Note:** a test window was closed by the user while I was testing (I had been bringing it to the foreground). From now on no foreground/mouse/keyboard tests while the user is at the computer — DevTools-only checks.
- **Next step:** 2.4, then finish 2.3.

### 2.2 kb-privacy: adblock-rust request blocking — DONE (2026-11-12)
- **Done:** New crate `kb-privacy` (adblock 0.13, MPL-2.0, built without the `single-thread` feature so the engine is `Send + Sync`): `Blocker` (`RwLock<Option<Engine>>`, `load_lists`, `load_dir` for `*.txt`, `check(url, source, type)` → block / rewritten URL), `ResourceType`, `is_filterable`; 6 tests (domain/type/path rules, exceptions, third-party, documents, dir loading). Shell: `content_blocking.rs` — `RequestHandler::resource_request_handler` → `on_before_resource_load` asks the global blocker for every sub-resource (CEF resource type mapped), cancels blocked ones, applies `$removeparam` rewrites, counts blocked requests per browser (reset on main-frame navigation) → `TabState.blocked`. Lists load from `<profile>/filters/*.txt` at startup. Verified live with a test list: blocked image → `ERR_ABORTED`, unblocked → normal DNS path, counter = 1.
- **Decision:** main-frame navigations are never blocked (adblock-rust would block documents of listed domains; a typed address always loads — strict mode can come with the per-site shield in 2.4). No lists are bundled: EasyList & co. are GPL/CC-BY-SA, so they are downloaded at runtime (2.3).
- **Also:** package workflow — `.deb` builds in CI; AppImage step needed `desktop-file-utils` (added).
- **Next step:** 2.3.

### 2.1 Google services off + zero unwanted connections — DONE (2026-11-12)
- **Done:** `kb-shell/src/privacy_flags.rs`: switches (`disable-background-networking`, `disable-component-update`, `disable-sync`, `no-pings`, `metrics-recording-only`, …), `--disable-features` list, `--host-resolver-rules` that make Google update/telemetry hosts unresolvable, global + profile prefs (component updates, search suggestions, Safe Browsing, translate, spelling service, network prediction = never, DoH auto-upgrade off, DNT). Applied in `App::on_before_command_line_processing` (browser process) and on context init. `KB_PROFILE_DIR` overrides the profile dir. `scripts/check-connections.mjs`: fresh profile, idle on `kb://newtab/`, graceful close, NetLog analysis — FAIL on requests to non-blocked public hosts or successful public socket connects; INFO for resolver-blocked attempts and connect-only probes.
- **Findings (CEF 156, fresh profile, 30 s idle, before → after):** `update.googleapis.com` + `edgedl.me.gvt1.com` (component updater — ignores `--disable-component-update`, runs in the system network context, invisible to CEF request handlers) → blocked by resolver rule, no packet leaves; `www.google.com/async/folae` (AI Mode eligibility) → gone after disabling `AimEnabled`, `AimServerEligibilityEnabled`, `AimServerRequestOnStartupEnabled` (names from Chromium source); `[2001:4860:4860::8888]:443` → Chromium IPv6 reachability probe (UDP connect, no data, fails with -109), reported as info. Result: `OK: no unwanted connections`.
- **Open:** run the check in CI (needs a desktop session: Windows runner works, Linux needs Xvfb) and the 5-minute window from §4.1 — tracked for 2.11 (performance/CI scripts).
- **Next step:** 2.2.

### 1.14 Packaging — DONE (2026-11-12)
- **Done:** `scripts/package-windows.ps1` (release build → `Kendime-Brow-<ver>-windows-x64.zip` + `.sha256`; app folder with CEF runtime, LICENSE, CEF credits, `install.ps1` / `uninstall.ps1` per-user to `%LOCALAPPDATA%\Programs\Kendime-Brow` + Start Menu shortcut), `scripts/package-linux.sh` (`.deb` installing to `/opt/kendime-brow` + `/usr/bin` symlink + desktop entry; AppImage via appimagetool), `scripts/install-dev.ps1` (`just install-dev`: Start Menu shortcut to the debug build), `just package`. `.github/workflows/package.yml` (manual / `v*` tags) builds both and uploads artifacts. Shared AppUserModelID `EmreYigitTasdelen.KendimeBrow` set in the browser and helper processes (one taskbar icon). Subprocesses now use `kb-shell-helper` (`browser_subprocess_path`). Verified: Windows zip built locally (188 MB zip, 414 MB unpacked) and in CI; the unpacked app starts and the bridge works.
- **Open:** signed MSI with the updater (2.12); real app icon; Linux package job fixed (missing exec bit) — result in the next CI run.
- **Next step:** 2.1.

### 1.13 Download manager (basic) — DONE (2026-11-12)
- **Done:** `kb-platform::files` (downloads dir incl. `XDG_DOWNLOAD_DIR`, `sanitize_file_name` against path traversal / reserved names / control chars, `unique_path` → `name (1).ext`, `reveal` in the file manager; tested). `kb-shell/src/downloads.rs` + `DownloadHandler`: saves without a dialog, tracks progress/speed/status, keeps the item callback for cancel/pause/resume. Bridge: `cancel-/pause-/resume-/reveal-download`, `clear-downloads`; `State.downloads`. UI: ↓ button with active-count badge, `kb://downloads/` page (progress bar, actions), action `downloads.open`. Verified: a `<a download>` click saved `kb-download-test.txt` (26 B) to `~/Downloads`, status `complete`, page rendered (test file removed afterwards).
- **Note:** Chromium blocks a second automatic download from the same page without a new user gesture ("multiple downloads" protection) — expected, not a bug. Download history is per session (not persisted yet); checksums / "open with" rules come in 3C.7.
- **Next step:** 1.14.

### 1.12 Profile, settings, kb.sqlite — DONE (2026-11-12)
- **Done:** `kb-shell/src/store.rs` (rusqlite, bundled SQLite, WAL): `settings` (whitelisted keys `search_url`, `restore_session`, `history_enabled`), `bookmarks`, `history` (visit count, title filled in later), `session`; internal `kb://` / `about:` pages never stored; 6 tests incl. file persistence. Profile dir `<data dir>/profiles/default` (`kb.sqlite` + CEF `cef/` cache, persistent session cookies). Shell: history on URL change, session saved on every state change (not while the window is closing), startup restores the session (or opens the new tab page), address bar uses the configured search engine. Bridge: `toggle-bookmark`, `profile`, `set-setting`, `delete-bookmark`, `clear-history`; `TabState.bookmarked`. UI: ☆/★ button, `kb://settings/` (search engine, restore session, history on/off, bookmarks, searchable history, clear). Actions `bookmark.toggle`, `settings.open`. Verified end to end: bookmark + history recorded, graceful close, relaunch restores the example.org tab; settings page screenshot OK.
- **Decision:** session restore loads all tabs for now; lazy "sleeping" tabs come with 2.8. Search engine choices: DuckDuckGo (default), Startpage, Brave Search, Mojeek.
- **Next step:** 1.13.

### 1.11 Action registry + command palette — DONE (2026-11-12)
- **Done:** `kb-shell/src/actions.rs` registers the shell actions in `kb-actions` (`tab.new/close/next/prev`, `nav.back/forward/reload/stop`) and maps each id to a bridge command. New bridge commands `actions` (search), `run-action`, `set-palette`, `close-active-tab`, `cycle-tab`; `State.palette`. UI: ⌘ button + `Palette.tsx` (search, ↑/↓, Enter, Esc, click). While open the strip grows to 420 px. Shortcut Ctrl+Shift+P added to the accelerator table (same manual-check caveat as 1.6). Verified with real input: open palette, filter, run "New tab" and "Close tab".
- **Fixed on the way:** (1) a new tab BrowserView was painted over the UI strip → tab views are now inserted at child index 0 so the strip stays on top; layout is re-applied once more in a posted UI task after CEF's own layout pass. (2) Ubuntu CI: second Linux-only type difference (`cef_event_flags_t` is u32 there).
- **Next step:** 1.12.

### 1.10 Module host launcher (Windows) — DONE (2026-11-12), AppContainer pending
- **Done:** `kb-platform::sandbox`: `spawn(SpawnSpec)` with a restricted primary token (`DISABLE_MAX_PRIVILEGE`, LOW integrity) via `CreateProcessAsUserW` (suspended → assigned to a Job Object → resumed). Job: kill-on-close, active process limit, optional per-process memory limit, die on unhandled exception. Windows pipes now have an explicit SDDL `D:P(A;;GA;;;<user SID>)S:(ML;;NW;;;LW)` (closes the 1.9 follow-up; low-IL hosts can connect, other users cannot). `kb-ipc::launch`: fresh endpoint + token per host (env `KB_IPC_ENDPOINT` / `KB_IPC_TOKEN`), 10 s connect timeout, `wait_exit`, `RestartPolicy` (restart non-zero exits up to 3×). Test host `kb-echo-host`. Integration tests: sandboxed round trip; low-IL host cannot write a user file; crash detected → restart works; clean exit not restarted.
- **Also fixed:** Ubuntu CI had been red since 1.6: `add_custom_scheme` option type is `u32` on Linux vs `i32` on Windows → `options as _`.
- **Decision / open:** AppContainer (no network / no file access by capability) is not done yet — the low-IL token already blocks writes; network blocking for kb-doc/kb-media must be added before those hosts parse untrusted files (Phase 3). Token passed via environment: readable by same-user processes, accepted (same trust boundary). Linux/macOS spawn is unsandboxed until 1.10b.
- **Next step:** 1.11.

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
