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

function Warm-JdtlsConfiguration {
    param([string]$DestRoot)
    $configReady = Join-Path $DestRoot "configuration\org.eclipse.osgi"
    if (Test-Path $configReady) {
        return
    }

    $javaCandidates = @(
        (Join-Path $Root "resources\jdk-windows-x64\bin\java.exe")
    )
    $java = $javaCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $java) {
        if ($env:JAVA_HOME -and (Test-Path (Join-Path $env:JAVA_HOME "bin\java.exe"))) {
            $java = Join-Path $env:JAVA_HOME "bin\java.exe"
        } elseif (Get-Command java -ErrorAction SilentlyContinue) {
            $java = (Get-Command java).Source
        }
    }
    if (-not $java) {
        Write-Host "jdtls warm-start skipped: JDK 21 not found (run scripts/vendor-jdk-windows.ps1)" -ForegroundColor Yellow
        return
    }

    $jar = Get-ChildItem (Join-Path $DestRoot "plugins\org.eclipse.equinox.launcher_*.jar") -ErrorAction SilentlyContinue |
        Select-Object -First 1 -ExpandProperty FullName
    if (-not $jar) {
        Write-Host "jdtls warm-start skipped: equinox launcher jar missing" -ForegroundColor Yellow
        return
    }

    $config = Join-Path $DestRoot "config_win"
    $warmData = Join-Path $env:TEMP ("reaper-jdtls-warm-" + [guid]::NewGuid().ToString())
    New-Item -ItemType Directory -Force -Path $warmData | Out-Null
    Write-Host "Warming bundled jdtls configuration..."
    $args = @(
        "-Declipse.application=org.eclipse.jdt.ls.core.id1",
        "-Dosgi.bundles.defaultStartLevel=4",
        "-Declipse.product=org.eclipse.jdt.ls.core.product",
        "-Dosgi.checkConfiguration=true",
        "-Dosgi.sharedConfiguration.area=$($config -replace '\\', '/')",
        "-Dosgi.sharedConfiguration.area.readOnly=true",
        "-Dosgi.configuration.cascaded=true",
        "-Xms256m",
        "--add-modules=ALL-SYSTEM",
        "--add-opens", "java.base/java.util=ALL-UNNAMED",
        "--add-opens", "java.base/java.lang=ALL-UNNAMED",
        "-jar", $jar,
        "-data", $warmData
    )
    $proc = Start-Process -FilePath $java -WorkingDirectory $DestRoot -ArgumentList $args -PassThru -WindowStyle Hidden
    try {
        for ($i = 0; $i -lt 180; $i++) {
            if (Test-Path $configReady) {
                Write-Host "Bundled jdtls configuration ready"
                return
            }
            if ($proc.HasExited) {
                Write-Host "jdtls warm-start exited before configuration was ready (exit $($proc.ExitCode))" -ForegroundColor Yellow
                return
            }
            Start-Sleep -Seconds 1
        }
        Write-Host "jdtls warm-start timed out before configuration was ready" -ForegroundColor Yellow
    } finally {
        if (-not $proc.HasExited) {
            Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        }
        Remove-Item -Recurse -Force $warmData -ErrorAction SilentlyContinue
    }
}

if ((Test-Path $Marker) -and ((Get-Content $Marker -Raw).Trim() -eq $Expected)) {
    if (Test-Path (Join-Path $Dest "plugins")) {
        Warm-JdtlsConfiguration -DestRoot $Dest
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
    Warm-JdtlsConfiguration -DestRoot $Dest
    Write-Host "jdtls $Expected -> $Dest"
}
finally {
    if (Test-Path $Tmp) {
        Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
    }
}
