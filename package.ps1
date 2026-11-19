# Builds the staff hand-out files into .\dist :
#   nhl-dxf-cleaner-setup-<version>.exe     one-file installer (recommended)
#   nhl-dxf-cleaner-<version>-portable.zip  no-install version (just unzip and run)
#
# Usage:  powershell -ExecutionPolicy Bypass -File .\package.ps1

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

function Invoke-Checked([string]$what, [scriptblock]$cmd) {
    & $cmd
    if ($LASTEXITCODE -ne 0) { throw "$what failed (exit code $LASTEXITCODE)" }
}

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
Write-Host "Packaging DXF Cleaner $version"

Invoke-Checked 'cargo test' { cargo test --release --quiet }
Invoke-Checked 'app build' { cargo build --release }

$app = Join-Path $PSScriptRoot 'target\release\dxf-cleaner.exe'
$cli = Join-Path $PSScriptRoot 'target\release\dxf-clean.exe'
$env:DXF_APP_EXE = $app
$env:DXF_APP_VERSION = $version
Invoke-Checked 'installer tests' { cargo test --release --quiet --manifest-path installer\Cargo.toml }
Invoke-Checked 'installer build' { cargo build --release --manifest-path installer\Cargo.toml }

$dist = Join-Path $PSScriptRoot 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
$setup = Join-Path $dist "nhl-dxf-cleaner-setup-$version.exe"
Copy-Item installer\target\release\dxf-cleaner-setup.exe $setup -Force

# Portable zip
$stage = Join-Path $dist "DXF Cleaner $version"
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $stage | Out-Null
Copy-Item $app (Join-Path $stage 'DXF Cleaner.exe')
Copy-Item $cli (Join-Path $stage 'dxf-clean.exe')
@"
DXF Cleaner $version
======================

Cleans DXF files exported from Adobe Illustrator (and similar programs) so the
cutting software gets continuous straight lines and true arcs.

How to use
  1. Double-click "DXF Cleaner.exe" (or drag a .dxf file onto it).
  2. Click "Open DXF" and choose the file.
  3. Click "Clean Up" and compare BEFORE (left) with AFTER (right).
  4. Click "Export DXF" - it saves <name>_clean.dxf next to the original.

Tip: to install it with a desktop shortcut instead, run
nhl-dxf-cleaner-setup-$version.exe.

If Windows shows "Windows protected your PC", click "More info" and then
"Run anyway" (the program is not code-signed).
"@ | Set-Content -Encoding UTF8 (Join-Path $stage 'READ ME.txt')
$zip = Join-Path $dist "nhl-dxf-cleaner-$version-portable.zip"
Remove-Item $zip -Force -ErrorAction SilentlyContinue
Compress-Archive -Path $stage -DestinationPath $zip
Remove-Item $stage -Recurse -Force

Get-ChildItem $dist | Select-Object Name, @{ n = 'Size (MB)'; e = { [math]::Round($_.Length / 1MB, 1) } } | Format-Table -AutoSize
