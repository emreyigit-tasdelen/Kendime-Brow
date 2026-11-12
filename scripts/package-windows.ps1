# Builds a release and packages it as a portable zip with a per-user installer (task 1.14).
#
#   powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1 [-SkipBuild]
#
# Output: target/package/Kendime-Brow-<version>-windows-x64.zip
# The zip contains the app folder plus install.ps1 / uninstall.ps1 (per-user, no admin rights:
# %LOCALAPPDATA%\Programs\Kendime-Brow + Start Menu shortcut).
# A signed MSI comes with the auto-updater (2.12).

param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches.Groups[1].Value

if (-not $SkipBuild) {
    Push-Location ui
    npm ci
    npm run build
    Pop-Location
    cargo build --release -p kb-shell -p kb-doc
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
}

$release = Join-Path $root 'target/release'
$pkgRoot = Join-Path $root 'target/package'
$stage = Join-Path $pkgRoot 'Kendime-Brow'
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force -Path $stage | Out-Null

# App binaries + the CEF runtime that cef-dll-sys copied next to them.
$files = @(
    'kendime-brow.exe', 'kb-shell-helper.exe', 'kb-doc-host.exe',
    'libcef.dll', 'chrome_elf.dll', 'd3dcompiler_47.dll', 'dxcompiler.dll', 'dxil.dll',
    'vk_swiftshader.dll', 'vk_swiftshader_icd.json', 'vulkan-1.dll',
    'icudtl.dat', 'v8_context_snapshot.bin',
    'chrome_100_percent.pak', 'chrome_200_percent.pak', 'resources.pak'
)
foreach ($f in $files) {
    $src = Join-Path $release $f
    if (Test-Path $src) { Copy-Item $src $stage }
    elseif ($f -in @('kendime-brow.exe', 'kb-shell-helper.exe', 'libcef.dll')) { throw "missing $f" }
}
# PDFium (scripts/fetch-pdfium.ps1 puts it in target/pdfium/bin); without it the PDF viewer is unavailable.
$pdfium = Join-Path $root 'target/pdfium/bin/pdfium.dll'
if (Test-Path $pdfium) { Copy-Item $pdfium $stage } else { Write-Warning 'pdfium.dll not found - run scripts/fetch-pdfium.ps1 first' }
Copy-Item -Recurse (Join-Path $release 'locales') $stage
Copy-Item LICENSE, README.md $stage
if (Test-Path (Join-Path $release 'CREDITS.html')) {
    Copy-Item (Join-Path $release 'CREDITS.html') (Join-Path $stage 'THIRD-PARTY-CEF-CREDITS.html')
}

@'
# Per-user install: copies the app to %LOCALAPPDATA%\Programs\Kendime-Brow and adds a Start Menu
# shortcut. No administrator rights needed.
$ErrorActionPreference = 'Stop'
$target = Join-Path $env:LOCALAPPDATA 'Programs\Kendime-Brow'
if (Get-Process kendime-brow -ErrorAction SilentlyContinue) { throw 'Close Kendime-Brow first.' }
if (Test-Path $target) { Remove-Item -Recurse -Force $target }
New-Item -ItemType Directory -Force -Path $target | Out-Null
Copy-Item -Recurse -Force (Join-Path $PSScriptRoot '*') $target -Exclude 'install.ps1'
$menu = Join-Path ([Environment]::GetFolderPath('Programs')) 'Kendime-Brow.lnk'
$sh = (New-Object -ComObject WScript.Shell).CreateShortcut($menu)
$sh.TargetPath = Join-Path $target 'kendime-brow.exe'
$sh.WorkingDirectory = $target
$sh.Save()
"Installed to $target"
'@ | Set-Content -Encoding UTF8 (Join-Path $stage 'install.ps1')

@'
# Removes the per-user install and the Start Menu shortcut. The profile
# (%LOCALAPPDATA%\Kendime-Brow) is kept; delete it yourself to remove all data.
$ErrorActionPreference = 'Stop'
$target = Join-Path $env:LOCALAPPDATA 'Programs\Kendime-Brow'
if (Get-Process kendime-brow -ErrorAction SilentlyContinue) { throw 'Close Kendime-Brow first.' }
Remove-Item -Recurse -Force $target -ErrorAction SilentlyContinue
Remove-Item -Force (Join-Path ([Environment]::GetFolderPath('Programs')) 'Kendime-Brow.lnk') -ErrorAction SilentlyContinue
"Removed $target"
'@ | Set-Content -Encoding UTF8 (Join-Path $stage 'uninstall.ps1')

$zip = Join-Path $pkgRoot "Kendime-Brow-$version-windows-x64.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path $stage -DestinationPath $zip
$hash = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
"$hash  $(Split-Path -Leaf $zip)" | Set-Content -Encoding ASCII "$zip.sha256"
"Package: $zip"
"SHA-256: $hash"
