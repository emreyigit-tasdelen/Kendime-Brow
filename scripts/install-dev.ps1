# Development install (Windows, task 1.14): a Start Menu shortcut "Kendime-Brow (dev)" that runs
# the current build, so every `cargo build` is picked up immediately.
# Usage: install-dev.ps1 [-Profile debug|release]. Target dir: $env:CARGO_TARGET_DIR, else the
# `target-dir` from ~/.cargo/config.toml, else <repo>/target.
param([string]$Profile = 'release')
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$target = $env:CARGO_TARGET_DIR
if (-not $target) {
  $cfg = Join-Path $HOME '.cargo\config.toml'
  if (Test-Path $cfg) {
    $m = Select-String -Path $cfg -Pattern '^\s*target-dir\s*=\s*"(.+)"' | Select-Object -First 1
    if ($m) { $target = $m.Matches[0].Groups[1].Value -replace '\\', '\' }
  }
}
if (-not $target) { $target = Join-Path $root 'target' }
$exe = Join-Path $target "$Profile\kendime-brow.exe"
if (-not (Test-Path $exe)) { throw "Build first: $exe not found" }
$lnk = Join-Path ([Environment]::GetFolderPath('Programs')) 'Kendime-Brow (dev).lnk'
$sh = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk)
$sh.TargetPath = $exe
$sh.WorkingDirectory = Split-Path $exe
$sh.IconLocation = (Join-Path $root 'crates\kb-shell\assets\icon.ico') + ',0'
$sh.Save()
"Shortcut: $lnk -> $exe"
