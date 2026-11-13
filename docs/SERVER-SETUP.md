# Headscale server setup (task 5.1)

Kendime-Brow's mesh (`kb-mesh`, `tsnet`) and the companion connect to **your own** Headscale server.
This guide installs it on a small Linux VPS with a public IP. **Credentials never go into this repository:**
pre-auth keys, API keys and private keys stay on the server or in the Kendime-Brow vault.

## 1. What you need
- A VPS (1 vCPU / 1 GB RAM is plenty) running a current Debian/Ubuntu LTS, a public IPv4 address.
- A DNS name pointing to it (`<headscale.example.org>`).
- Ports: TCP 443 (HTTPS: control + embedded DERP), UDP 3478 (STUN). Nothing else is exposed.
- Nothing is opened on the office router: both ends connect outward.

## 2. Install
1. Download the `.deb` for your architecture from the Headscale GitHub releases page and **verify its checksum**
   against the release's checksum file before installing.
2. `sudo apt install ./headscale_<version>_linux_<arch>.deb`
3. Copy `deploy/headscale/config.yaml.example` to `/etc/headscale/config.yaml`, replace the placeholders and diff it
   with the `config-example.yaml` shipped for your version.
4. `sudo systemctl enable --now headscale` and check `sudo systemctl status headscale`.

## 3. TLS reverse proxy
Headscale listens on 127.0.0.1:8080 only. Put a TLS terminator in front (Caddy shown; nginx works too):

```
<headscale.example.org> {
    reverse_proxy 127.0.0.1:8080
}
```
Caddy obtains and renews the certificate automatically. The proxy must pass WebSocket/HTTP upgrade (Caddy does by default),
which the embedded DERP and the Tailscale control protocol need.

## 4. Hardening
- Firewall: allow only 22 (restrict by source if possible), 443/tcp and 3478/udp; deny the rest (`ufw default deny incoming`).
- SSH: keys only, `PasswordAuthentication no`, `PermitRootLogin no`. Enable unattended security upgrades.
- Keep the metrics and gRPC listeners on 127.0.0.1 (the example does).
- Run Headscale as its own unprivileged user (the package does) and keep `/var/lib/headscale` mode 0750.
- Use **pre-auth keys that are single-use and expire quickly** (see section 6); never reuse them.
- Do not use Tailscale's public DERP map (the example sets `derp.urls: []`).
- Restrict who can reach whom with an ACL policy (task 5.10 will ship a template): by default allow only your
  browser nodes to reach the companion nodes on the ports you use (RDP 3389, SSH 22, SFTP).

## 5. Users and nodes
```
sudo headscale users create <name>
sudo headscale users list
sudo headscale nodes list
```

## 6. Enrolling a device (pre-auth key)
```
sudo headscale preauthkeys create --user <id> --expiration 15m
```
(Use `--reusable` only deliberately. Do not paste the key into chat, tickets or this repository.)
Kendime-Brow's wizard (task 5.4) and the companion installer (5.8) ask for the server URL and the key, use it once,
and store the resulting node state in the vault. Remove stale nodes with `sudo headscale nodes delete -i <id>`.

## 7. Backup and restore
What matters: `/etc/headscale/config.yaml` and `/var/lib/headscale/` (SQLite database + the noise and DERP private keys).
Losing the noise key means every node must re-enroll.

Backup (daily, encrypted, stored off the server):
```
sudo systemctl stop headscale        # or use sqlite3 ".backup" for a hot copy
sudo tar czf - /etc/headscale /var/lib/headscale | age -r <your-age-public-key> > headscale-$(date +%F).tar.gz.age
sudo systemctl start headscale
```
Restore on a fresh host: install the same Headscale version, restore the two directories (keep ownership `headscale:headscale`),
start the service, and run `headscale nodes list` to confirm. Test a restore at least once before relying on it.

## 8. Updating
Read the release notes first (config and database migrations happen on start). Take a backup, install the new `.deb`,
diff the config against the new `config-example.yaml`, restart, and check `headscale nodes list`.

## 9. Verifying
```
curl -sS https://<headscale.example.org>/health     # {"status":"pass"}
sudo ss -ltnup | grep -E ':(8080|9090|50443|3478)'  # 8080/9090/50443 on 127.0.0.1, 3478 on 0.0.0.0
```
