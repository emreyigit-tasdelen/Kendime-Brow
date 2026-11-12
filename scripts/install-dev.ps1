# Development install (Windows, task 1.14): a Start Menu shortcut "Kendime-Brow (dev)" that runs
# the current debug build from target/debug, so every `cargo build` is picked up immediately.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root 'target\debug\kendime-brow.exe'
if (-not (Test-Path $exe)) { throw "Build first: $exe not found" }
$lnk = Join-Path ([Environment]::GetFolderPath('Programs')) 'Kendime-Brow (dev).lnk'
$sh = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk)
$sh.TargetPath = $exe
$sh.WorkingDirectory = Split-Path $exe
$sh.Save()
"Shortcut: $lnk -> $exe"
