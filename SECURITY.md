# Security and Sensitive Data Policy

This repository is public. Nothing sensitive may ever be committed.

## What counts as sensitive

- Tokens, API keys, passwords, signing keys (e.g. the `minisign` release key)
- SSH, WireGuard, Tailscale/Headscale private keys; Headscale `config.yaml`, `db.sqlite`, `noise_private.key`
- Server IP addresses, real domain names, office network details
- Personal email addresses, phone numbers, any personal data
- Network captures and session logs (`*.pcap`, `*.pcapng`, `*.har`)

Sensitive values live outside the repo (`.env`, OS keyring, `kb-vault`). The repo only contains
`*.example` templates with placeholder values.

## Protection layers

| # | Layer | Where |
|---|---|---|
| 1 | Secrets stored outside the repo, `*.example` templates only | Convention |
| 2 | `.gitignore` excludes keys, `.env`, captures, profiles, server files | Local |
| 3 | `pre-commit` hook: gitleaks scans staged changes (`.gitleaks.toml`, incl. personal email rule) | Local |
| 4 | `commit-msg` hook: blocks AI attribution lines and non-noreply author emails | Local |
| 5 | GitHub secret scanning + push protection | GitHub |
| 6 | CI: gitleaks over the full history + author/attribution check (`.github/workflows/secret-scan.yml`) | GitHub Actions |
| 7 | GitHub account: "Block command line pushes that expose my email" | GitHub account |

### Setup after cloning

```sh
git config core.hooksPath .githooks
git config user.email "<id>+<username>@users.noreply.github.com"
```

gitleaks must be installed (`winget install Gitleaks.Gitleaks`, or from the gitleaks releases page);
the hook blocks commits if it is missing.

## If something leaks

1. **Revoke / rotate the secret immediately.** Assume it is compromised — removing it from history
   does not undo copies, forks or caches.
2. Remove it from history with `git filter-repo` and force push.
3. Ask GitHub Support to purge cached views if needed.
4. Record the incident (without the secret) in `PROGRESS.md`.

## Reporting a vulnerability

Please open a private security advisory on GitHub (Security → Report a vulnerability) instead of a public issue.
