# Download java-debug plugin JAR for bundled Java debugging on Windows.
param(
    [string]$Version = "0.59.0"
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$Dest = Join-Path $Root "resources\debug-adapters-windows-x64"
$CacheDir = Join-Path $Root "resources\.cache"
$Vsix = "vscjava.vscode-java-debug-$Version.vsix"
$Url = "https://open-vsx.org/api/vscjava/vscode-java-debug/$Version/file/$Vsix"
$Cache = Join-Path $CacheDir $Vsix
$Marker = Join-Path $Dest "java-debug\.vendor-version"
$JarDir = Join-Path $Dest "java-debug\server"

if ((Test-Path $Marker) -and ((Get-Content $Marker -Raw).Trim() -eq $Version)) {
    Write-Host "java-debug $Version already vendored at $Dest"
    exit 0
}

New-Item -ItemType Directory -Force -Path $Dest, $CacheDir, $JarDir | Out-Null
if (-not (Test-Path $Cache)) {
    Write-Host "Downloading java-debug $Version..."
    Invoke-WebRequest -Uri $Url -OutFile $Cache -UseBasicParsing
}

$Tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("reaper-java-debug-" + [guid]::NewGuid().ToString())
New-Item -ItemType Directory -Force -Path $Tmp | Out-Null
try {
    $ZipCopy = Join-Path $Tmp "package.zip"
    Copy-Item $Cache $ZipCopy
    Expand-Archive -Path $ZipCopy -DestinationPath $Tmp -Force
    $Server = Join-Path $Tmp "extension\server"
    if (-not (Test-Path $Server)) {
        throw "VSIX missing extension/server/ folder"
    }
    Get-ChildItem $JarDir -ErrorAction SilentlyContinue | Remove-Item -Force
    Copy-Item (Join-Path $Server "*") $JarDir -Recurse -Force
    $jar = Get-ChildItem $JarDir -Filter "com.microsoft.java.debug.plugin-*.jar" | Select-Object -First 1
    if (-not $jar) {
        throw "com.microsoft.java.debug.plugin jar missing after vendor"
    }
    Set-Content -Path $Marker -Value $Version -NoNewline
    Write-Host "java-debug plugin $Version -> $($jar.FullName)"
}
finally {
    if (Test-Path $Tmp) {
        Remove-Item -Recurse -Force $Tmp
    }
}
