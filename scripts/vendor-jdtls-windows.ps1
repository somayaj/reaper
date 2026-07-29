# Download Eclipse JDT Language Server for bundled Java support on Windows.
param(
    [string]$Version = "1.60.0",
    [string]$Build = "202606262232"
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$Dest = Join-Path $Root "resources\jdtls"
$CacheDir = Join-Path $Root "resources\.cache"
$Tarball = "jdt-language-server-$Version-$Build.tar.gz"
$Cache = Join-Path $CacheDir $Tarball
$Url = "https://www.eclipse.org/downloads/download.php?file=/jdtls/milestones/$Version/$Tarball"
$Marker = Join-Path $Dest ".vendor-version"
$Expected = "$Version-$Build"

if ((Test-Path $Marker) -and ((Get-Content $Marker -Raw).Trim() -eq $Expected)) {
    if (Test-Path (Join-Path $Dest "plugins")) {
        Write-Host "jdtls $Expected already vendored at $Dest"
        exit 0
    }
}

New-Item -ItemType Directory -Force -Path $CacheDir | Out-Null
if (-not (Test-Path $Cache)) {
    Write-Host "Downloading jdtls $Expected..."
    Invoke-WebRequest -Uri $Url -OutFile $Cache -UseBasicParsing
}

$Tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("reaper-jdtls-" + [guid]::NewGuid().ToString())
New-Item -ItemType Directory -Force -Path $Tmp | Out-Null
try {
    if (Test-Path $Dest) {
        Remove-Item -Recurse -Force $Dest
    }
    New-Item -ItemType Directory -Force -Path $Dest | Out-Null
    tar -xzf $Cache -C $Dest
    if (-not (Test-Path (Join-Path $Dest "config_win"))) {
        throw "jdtls missing config_win after extract"
    }
    Set-Content -Path $Marker -Value $Expected -NoNewline
    Write-Host "jdtls $Expected -> $Dest"
}
finally {
    if (Test-Path $Tmp) {
        Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
    }
}
