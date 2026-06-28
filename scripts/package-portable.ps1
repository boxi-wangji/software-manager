# Portable folder build (data beside exe)
param(
    [switch]$WithWebView2,
    [switch]$Relaunch
)

$ErrorActionPreference = "Stop"
$Root = Split-Path $PSScriptRoot -Parent
$ReleaseDir = Join-Path $Root "release\software-manager"
$DesktopDir = Join-Path ([Environment]::GetFolderPath("Desktop")) "software-manager"
$Rcedit = Join-Path $PSScriptRoot "rcedit-x64.exe"
$IconIco = Join-Path $Root "src-tauri\icons\icon.ico"

function Set-ExeIcon {
    param(
        [string]$ExePath,
        [string]$IconPath
    )
    if (-not (Test-Path $Rcedit)) { throw "missing $Rcedit" }
    if (-not (Test-Path $IconPath)) { throw "missing icon $IconPath" }
    $tmpExe = Join-Path $env:TEMP ("software-manager-iconpatch-{0}.exe" -f [guid]::NewGuid().ToString("n"))
    try {
        Copy-Item $ExePath $tmpExe -Force
        & $Rcedit $tmpExe --set-icon $IconPath
        if ($LASTEXITCODE -ne 0) { throw "rcedit failed for $ExePath" }
        Copy-Item $tmpExe $ExePath -Force
    } finally {
        if (Test-Path $tmpExe) { Remove-Item $tmpExe -Force -ErrorAction SilentlyContinue }
    }
}

function Stop-ProcessTree {
    param(
        [int]$ProcessId,
        [string]$Name = "process"
    )
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = "SilentlyContinue"
    try {
        if (-not (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)) { return $true }

        Write-Host "      stopping $Name pid $ProcessId..."
        Stop-Process -Id $ProcessId -Force -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 250

        if (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue) {
            $null = & taskkill.exe /F /PID $ProcessId /T 2>&1
            Start-Sleep -Milliseconds 400
        }

        if (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue) {
            $isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
                [Security.Principal.WindowsBuiltInRole]::Administrator
            )
            if (-not $isAdmin) {
                Write-Host "      requesting elevated stop for pid $ProcessId..."
                Start-Process -FilePath "taskkill.exe" -ArgumentList "/F", "/PID", "$ProcessId", "/T" -Verb RunAs -Wait -WindowStyle Hidden | Out-Null
                Start-Sleep -Milliseconds 500
            }
        }

        return -not (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)
    } finally {
        $ErrorActionPreference = $prevEap
    }
}

function Stop-RunningApp {
    param(
        [string]$TargetDir = $DesktopDir
    )
    $names = @("software-manager")
    for ($round = 1; $round -le 6; $round++) {
        $found = $false
        foreach ($name in $names) {
            $procs = @(Get-Process -Name $name -ErrorAction SilentlyContinue)
            foreach ($proc in $procs) {
                $found = $true
                $stopped = Stop-ProcessTree -ProcessId $proc.Id -Name $name
                if (-not $stopped) {
                    Write-Warning "could not stop $name pid $($proc.Id); close it manually or rerun hot-update from an elevated terminal"
                }
            }
        }
        if (-not $found) { break }
        Start-Sleep -Milliseconds 700
    }
    if (Test-Path $TargetDir) {
        Get-ChildItem $TargetDir -File -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like "*.iconpatch.exe" -or $_.Name -eq "software-manager.update.exe" } |
            Remove-Item -Force -ErrorAction SilentlyContinue
    }
}

function Remove-ObsoleteIconFiles {
    param(
        [string]$IconsDir
    )
    if (-not (Test-Path $IconsDir)) { return }
    $keep = @("32x32.png", "128x128.png", "128x128@2x.png", "icon.png", "icon.ico")
    Get-ChildItem $IconsDir -File | Where-Object { $keep -notcontains $_.Name } | Remove-Item -Force
}

function Remove-ObsoletePortableFiles {
    param(
        [string]$TargetDir
    )
    $obsolete = @(
        "assets",
        "index.html",
        "tauri.svg",
        "vite.svg",
        "software-manager.ico",
        "software-manager.update.exe",
        "software-manager.exe.iconpatch.exe",
        "scripts\visual-target.log"
    )
    foreach ($item in $obsolete) {
        $path = Join-Path $TargetDir $item
        if (Test-Path $path) {
            Remove-Item $path -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
    Get-ChildItem $TargetDir -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -like "*.iconpatch.exe" } |
        Remove-Item -Force -ErrorAction SilentlyContinue
}

function Deploy-ToDesktop {
    param(
        [string]$SourceDir,
        [string]$TargetDir
    )
    Stop-RunningApp -TargetDir $TargetDir
    New-Item -ItemType Directory -Force -Path $TargetDir, (Join-Path $TargetDir "scripts"), (Join-Path $TargetDir "data") | Out-Null
    Remove-ObsoletePortableFiles -TargetDir $TargetDir

    $srcExe = Join-Path $SourceDir "software-manager.exe"
    $destExe = Join-Path $TargetDir "software-manager.exe"
    Copy-Item $srcExe $destExe -Force

    Copy-Item (Join-Path $SourceDir "scripts\run-wegame-ocr.ps1") (Join-Path $TargetDir "scripts") -Force
    Copy-Item (Join-Path $SourceDir "scripts\visual-target.ps1") (Join-Path $TargetDir "scripts") -Force

    if (Test-Path (Join-Path $SourceDir "icons")) {
        $iconsDest = Join-Path $TargetDir "icons"
        if (Test-Path $iconsDest) { Remove-Item $iconsDest -Recurse -Force }
        New-Item -ItemType Directory -Force -Path $iconsDest | Out-Null
        foreach ($name in @("32x32.png", "128x128.png", "128x128@2x.png", "icon.png", "icon.ico")) {
            Copy-Item (Join-Path $SourceDir "icons\$name") $iconsDest -Force
        }
        Remove-ObsoleteIconFiles -IconsDir $iconsDest
    }
    if (Test-Path (Join-Path $SourceDir "README.txt")) {
        Copy-Item (Join-Path $SourceDir "README.txt") (Join-Path $TargetDir "README.txt") -Force
    }
    if (Test-Path (Join-Path $SourceDir "WebView2")) {
        Copy-Item (Join-Path $SourceDir "WebView2") (Join-Path $TargetDir "WebView2") -Recurse -Force
    }
}

Write-Host "[1/4] generating icon..."
python (Join-Path $Root "scripts\generate-app-icon.py")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

if ($Relaunch) {
    Stop-RunningApp
}

Write-Host "[2/4] building..."
$env:CARGO_BUILD_JOBS = "2"
Push-Location $Root
npm run build:exe
if ($LASTEXITCODE -ne 0) { Pop-Location; exit $LASTEXITCODE }
Pop-Location

$ExeSrc = Join-Path $Root "src-tauri\target\release\software-manager.exe"
if (-not (Test-Path $ExeSrc)) { throw "missing $ExeSrc" }
Set-ExeIcon -ExePath $ExeSrc -IconPath $IconIco

Write-Host "[3/4] assembling portable folder..."
if (Test-Path $ReleaseDir) { Remove-Item $ReleaseDir -Recurse -Force }
New-Item -ItemType Directory -Force -Path $ReleaseDir, (Join-Path $ReleaseDir "data"), (Join-Path $ReleaseDir "scripts") | Out-Null

$ReleaseExe = Join-Path $ReleaseDir "software-manager.exe"
Copy-Item $ExeSrc $ReleaseExe -Force

$ScriptsSrc = Join-Path $Root "scripts"
$ScriptsDest = Join-Path $ReleaseDir "scripts"
Copy-Item (Join-Path $ScriptsSrc "run-wegame-ocr.ps1") $ScriptsDest -Force
Copy-Item (Join-Path $ScriptsSrc "visual-target.ps1") $ScriptsDest -Force

$IconsSrc = Join-Path $Root "src-tauri\icons"
if (Test-Path $IconsSrc) {
    $IconsDest = Join-Path $ReleaseDir "icons"
    New-Item -ItemType Directory -Force -Path $IconsDest | Out-Null
    foreach ($name in @("32x32.png", "128x128.png", "128x128@2x.png", "icon.png", "icon.ico")) {
        Copy-Item (Join-Path $IconsSrc $name) $IconsDest -Force
    }
    Remove-ObsoleteIconFiles -IconsDir $IconsDest
}

@"
software-manager portable
=========================

Folder layout:
  software-manager.exe   program
  scripts\               WeGame install / visual automation scripts
  data\                  config and package cache
    config.json          auto-created settings
    packages\            downloaded installers

Installed apps default to:
  %LOCALAPPDATA%\software-manager\apps

Copy the folder above to use. Installed software is kept separately from this folder.

Dev / hot-update:
  npm run hot-update

Console policy:
  All helper subprocesses must run hidden (no flashing cmd/powershell windows).
  See software-manager-notes.md section "隐藏命令行窗口".
"@ | Set-Content (Join-Path $ReleaseDir "README.txt") -Encoding UTF8

if ($WithWebView2) {
    Write-Host "      bundling WebView2 (~600MB)..."
    $WebView2Out = Join-Path $ReleaseDir "WebView2"
    $TempDir = Join-Path $env:TEMP "sm-webview2-pack"
    $CabUrl = "https://msedge.sf.dl.delivery.mp.microsoft.com/filestreamingservice/files/2943b6d1-31d1-42c5-8cfa-c2c31485974d/Microsoft.WebView2.FixedVersionRuntime.149.0.4022.98.x64.cab"
    New-Item -ItemType Directory -Force -Path $TempDir, $WebView2Out | Out-Null
    if (-not (Test-Path (Join-Path $WebView2Out "msedgewebview2.exe"))) {
        $CabFile = Join-Path $TempDir "webview2.cab"
        Invoke-WebRequest -Uri $CabUrl -OutFile $CabFile -UseBasicParsing
        $ExtractDir = Join-Path $TempDir "cab"
        if (Test-Path $ExtractDir) { Remove-Item $ExtractDir -Recurse -Force }
        New-Item -ItemType Directory -Force -Path $ExtractDir | Out-Null
        tar -xf $CabFile -C $ExtractDir
        $RuntimeDir = Get-ChildItem $ExtractDir -Directory | Select-Object -First 1
        Copy-Item (Join-Path $RuntimeDir.FullName "*") $WebView2Out -Recurse -Force
    }
}

Write-Host "[4/4] deploy to Desktop..."
if (-not (Test-Path $DesktopDir)) {
    Copy-Item $ReleaseDir $DesktopDir -Recurse -Force
} else {
    Deploy-ToDesktop -SourceDir $ReleaseDir -TargetDir $DesktopDir
}

$DesktopExe = Join-Path $DesktopDir "software-manager.exe"
$DesktopIco = Join-Path $DesktopDir "icons\icon.ico"
if (-not (Test-Path $DesktopIco)) { $DesktopIco = $IconIco }
Set-ExeIcon -ExePath $DesktopExe -IconPath $DesktopIco
ie4uinit.exe -show | Out-Null

$SizeMB = [math]::Round((Get-ChildItem $DesktopDir -Recurse | Measure-Object Length -Sum).Sum / 1MB, 1)
Write-Host "OK: $DesktopDir ($SizeMB MB)"

if ($Relaunch) {
    $ExePath = Join-Path $DesktopDir "software-manager.exe"
    Start-Process -FilePath $ExePath -Verb RunAs -WindowStyle Hidden
    Write-Host "Relaunched software-manager as administrator"
}
