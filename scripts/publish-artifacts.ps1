# Collect release outputs into dist/ with clear installer vs portable names.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$confPath = Join-Path $root 'src-tauri\tauri.conf.json'
$conf = Get-Content -Raw $confPath | ConvertFrom-Json
$version = $conf.version
$product = $conf.productName
$arch = 'x64'

$releaseDir = Join-Path $root 'src-tauri\target\release'
$nsisDir = Join-Path $releaseDir 'bundle\nsis'
$msiDir = Join-Path $releaseDir 'bundle\msi'
$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force -Path $dist | Out-Null

$exe = Join-Path $releaseDir 'screenbound.exe'
if (-not (Test-Path $exe)) {
    throw "Release binary not found: $exe"
}

$nsis = Get-ChildItem -Path $nsisDir -Filter '*-setup.exe' -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if (-not $nsis) {
    throw "NSIS installer not found under $nsisDir"
}

$msi = Get-ChildItem -Path $msiDir -Filter '*.msi' -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if (-not $msi) {
    throw "MSI installer not found under $msiDir"
}

$nsisOut = Join-Path $dist ("{0}_{1}_{2}-setup.exe" -f $product, $version, $arch)
$msiOut = Join-Path $dist ("{0}_{1}_{2}.msi" -f $product, $version, $arch)
$portableOut = Join-Path $dist ("{0}_{1}_{2}-portable.exe" -f $product, $version, $arch)

Copy-Item -Force $nsis.FullName $nsisOut
Copy-Item -Force $msi.FullName $msiOut
Copy-Item -Force $exe $portableOut

Write-Host ''
Write-Host 'Release artifacts in dist/:'
Write-Host ("  NSIS installer : {0} ({1:N2} MiB)" -f $nsisOut, ((Get-Item $nsisOut).Length / 1MB))
Write-Host ("  MSI installer  : {0} ({1:N2} MiB)" -f $msiOut, ((Get-Item $msiOut).Length / 1MB))
Write-Host ("  Portable exe   : {0} ({1:N2} MiB)" -f $portableOut, ((Get-Item $portableOut).Length / 1MB))
Write-Host ''
Write-Host 'Note: *-setup.exe and *.msi install the app. *-portable.exe is a no-install single binary (needs WebView2).'
