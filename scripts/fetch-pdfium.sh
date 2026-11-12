#!/usr/bin/env bash
# Downloads the pinned PDFium build (Linux x64) and verifies its SHA-256. Usage: scripts/fetch-pdfium.sh [dest]
set -euo pipefail
dest="${1:-target/pdfium}"
tag="chromium/8152"
asset="pdfium-linux-x64.tgz"
sha256="384ab5c51882408a143a21f9c14dcd2caf1f75280ae41c554d0489b46c3dba9f"
mkdir -p "$dest"
curl -sSfL -o "$dest/$asset" "https://github.com/bblanchon/pdfium-binaries/releases/download/$tag/$asset"
echo "$sha256  $dest/$asset" | sha256sum -c -
tar -xzf "$dest/$asset" -C "$dest"
echo "PDFium ready: $dest/lib (export PDFIUM_PATH=$dest/lib)"
