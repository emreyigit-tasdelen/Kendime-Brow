#!/usr/bin/env bash
# Builds a release and packages it as a .deb and an AppImage (task 1.14).
#
#   scripts/package-linux.sh [--skip-build]
#
# Output: target/package/kendime-brow_<version>_amd64.deb and Kendime-Brow-<version>-x86_64.AppImage
# The app lives in /opt/kendime-brow (deb) or inside the AppImage; CEF needs its files next to
# the executable, so the launcher `kendime-brow` is a symlink/wrapper into that folder.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)

if [[ "${1:-}" != "--skip-build" ]]; then
  (cd ui && npm ci && npm run build)
  cargo build --release -p kb-shell
fi

release=target/release
pkg=target/package
app="$pkg/kendime-brow"
rm -rf "$app" && mkdir -p "$app"

for f in kendime-brow kb-shell-helper libcef.so icudtl.dat v8_context_snapshot.bin \
         chrome_100_percent.pak chrome_200_percent.pak resources.pak \
         libEGL.so libGLESv2.so libvk_swiftshader.so vk_swiftshader_icd.json libvulkan.so.1 chrome-sandbox; do
  if [[ -e "$release/$f" ]]; then cp -a "$release/$f" "$app/"; fi
done
for required in kendime-brow kb-shell-helper libcef.so; do
  [[ -e "$app/$required" ]] || { echo "missing $required" >&2; exit 1; }
done
cp -a "$release/locales" "$app/"
cp LICENSE README.md "$app/"
[[ -e "$release/CREDITS.html" ]] && cp "$release/CREDITS.html" "$app/THIRD-PARTY-CEF-CREDITS.html"

desktop_entry() {
  cat <<EOF
[Desktop Entry]
Type=Application
Name=Kendime-Brow
Comment=Privacy-focused personal web browser
Exec=$1 %U
Icon=kendime-brow
Terminal=false
Categories=Network;WebBrowser;
MimeType=text/html;x-scheme-handler/http;x-scheme-handler/https;
StartupWMClass=kendime-brow
EOF
}

# ---------------------------------------------------------------- .deb
deb="$pkg/deb"
rm -rf "$deb"
mkdir -p "$deb/DEBIAN" "$deb/opt" "$deb/usr/bin" "$deb/usr/share/applications"
cp -a "$app" "$deb/opt/kendime-brow"
ln -s /opt/kendime-brow/kendime-brow "$deb/usr/bin/kendime-brow"
desktop_entry /usr/bin/kendime-brow > "$deb/usr/share/applications/kendime-brow.desktop"
size=$(du -sk "$deb/opt" | cut -f1)
cat > "$deb/DEBIAN/control" <<EOF
Package: kendime-brow
Version: $version
Section: web
Priority: optional
Architecture: amd64
Installed-Size: $size
Depends: libnss3, libatk-bridge2.0-0, libgtk-3-0, libxss1, libasound2 | libasound2t64, libgbm1, libxkbcommon0
Maintainer: Emre Yiğit Taşdelen <noreply@users.noreply.github.com>
Homepage: https://github.com/emreyigit-tasdelen/Kendime-Brow
Description: Privacy-focused, resource-efficient personal web browser (CEF + Rust)
EOF
dpkg-deb --root-owner-group --build "$deb" "$pkg/kendime-brow_${version}_amd64.deb"

# ---------------------------------------------------------------- AppImage
appdir="$pkg/AppDir"
rm -rf "$appdir"
mkdir -p "$appdir/usr"
cp -a "$app" "$appdir/usr/lib-kendime-brow"
cat > "$appdir/AppRun" <<'EOF'
#!/bin/sh
here="$(dirname "$(readlink -f "$0")")"
exec "$here/usr/lib-kendime-brow/kendime-brow" --no-sandbox "$@"
EOF
chmod +x "$appdir/AppRun"
desktop_entry kendime-brow > "$appdir/kendime-brow.desktop"
# 1x1 placeholder icon until the real icon exists.
printf '<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256"><rect width="256" height="256" rx="48" fill="#3b6fd8"/></svg>' > "$appdir/kendime-brow.svg"
if command -v appimagetool >/dev/null 2>&1; then
  ARCH=x86_64 appimagetool "$appdir" "$pkg/Kendime-Brow-${version}-x86_64.AppImage"
else
  echo "appimagetool not found: AppDir left in $appdir (CI downloads the tool)" >&2
fi

(cd "$pkg" && sha256sum ./*.deb ./*.AppImage 2>/dev/null > SHA256SUMS || true)
ls -la "$pkg"
