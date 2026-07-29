# Download Microsoft OpenJDK 21 for bundled jdtls runtime on Windows (project JDK stays separate).
param(
    [string]$Version = "21.0.8"
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$Dest = Join-Path $Root "resources\jdk-windows-x64"
$CacheDir = Join-Path $Root "resources\.cache"
$ZipName = "microsoft-jdk-$Version-windows-x64.zip"
$Cache = Join-Path $CacheDir $ZipName
$Url = "https://aka.ms/download-jdk/microsoft-jdk-$Version-windows-x64.zip"
$Marker = Join-Path $Dest ".vendor-version"
$Java = Join-Path $Dest "bin\java.exe"

if ((Test-Path $Marker) -and ((Get-Content $Marker -Raw).Trim() -eq $Version) -and (Test-Path $Java)) {
    Write-Host "JDK $Version already vendored at $Dest"
    exit 0
}

New-Item -ItemType Directory -Force -Path $CacheDir | Out-Null
if (-not (Test-Path $Cache)) {
    Write-Host "Downloading Microsoft OpenJDK $Version (windows-x64)..."
    Invoke-WebRequest -Uri $Url -OutFile $Cache -UseBasicParsing
}

$Tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("reaper-jdk-" + [guid]::NewGuid().ToString())
New-Item -ItemType Directory -Force -Path $Tmp | Out-Null
try {
    Expand-Archive -Path $Cache -DestinationPath $Tmp -Force
    $Extracted = Get-ChildItem $Tmp -Directory | Select-Object -First 1
    if (-not $Extracted) {
        throw "JDK zip extracted empty"
    }
    if (Test-Path $Dest) {
        Remove-Item -Recurse -Force $Dest
    }
    Move-Item $Extracted.FullName $Dest
    if (-not (Test-Path $Java)) {
        throw "java.exe missing after JDK vendor"
    }
    Set-Content -Path $Marker -Value $Version -NoNewline
    Write-Host "JDK $Version -> $Dest"
}
finally {
    if (Test-Path $Tmp) {
        Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
    }
}
