# Downloads the pinned PDFium build (bblanchon/pdfium-binaries, BSD-3) and verifies its SHA-256.
#   powershell -ExecutionPolicy Bypass -File scripts/fetch-pdfium.ps1 [-Dest target/pdfium]
# Afterwards: $env:PDFIUM_PATH = "<Dest>/bin"
param([string]$Dest = 'target/pdfium')
$ErrorActionPreference = 'Stop'
$tag = 'chromium/8152'
$asset = 'pdfium-win-x64.tgz'
$sha256 = 'cee2b0e9990bc67c0721774932961b9dbf2766046c10eec925acd4cfba6fdfe3'
New-Item -ItemType Directory -Force -Path $Dest | Out-Null
$file = Join-Path $Dest $asset
Invoke-WebRequest "https://github.com/bblanchon/pdfium-binaries/releases/download/$tag/$asset" -OutFile $file
$actual = (Get-FileHash -Algorithm SHA256 $file).Hash.ToLower()
if ($actual -ne $sha256) { Remove-Item $file; throw "SHA-256 mismatch: $actual" }
tar -xzf $file -C $Dest
"PDFium ready: $(Resolve-Path (Join-Path $Dest 'bin'))"
