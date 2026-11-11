# ui

`kb://` pages: TypeScript + SolidJS + Vite (architecture §3.4).

| Folder | Host | Page |
|---|---|---|
| `strip/` | `kb://ui/` | Window strip: tab row + toolbar/address bar |
| `newtab/` | `kb://newtab/` | Start page |
| `src/` | — | Components, `bridge.ts` (typed client for the `kbQuery` bridge) |

```sh
npm ci
npm run build   # → dist/, embedded into kb-shell (debug builds read dist/ from disk)
```

Pages run under a strict CSP (no inline scripts/styles, no external requests), so Vite's
module-preload polyfill is disabled. Shared bundles land in `dist/assets/` and are served under
every host (`kb://ui/assets/…`).
