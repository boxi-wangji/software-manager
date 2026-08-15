param(
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version = '0.1.0'
)

$ErrorActionPreference = 'Stop'

$sourceRoot = Split-Path -Parent $PSScriptRoot
$workspaceRoot = Split-Path -Parent $sourceRoot
$buildRoot = Join-Path $workspaceRoot '构建'
$env:CARGO_TARGET_DIR = Join-Path $buildRoot 'Rust'
$temporaryRoot = Join-Path $buildRoot '临时\Velopack'
$packageDirectory = Join-Path $temporaryRoot '软件管家'
$releaseDirectory = Join-Path $temporaryRoot '发布'
$installerDirectory = Join-Path $buildRoot '安装程序'
$projectFile = Join-Path $sourceRoot 'src-tauri\tauri.conf.json'
$executableSource = Join-Path $env:CARGO_TARGET_DIR 'release\software-manager.exe'
$iconFile = Join-Path $sourceRoot 'src-tauri\icons\icon.ico'
$rcedit = Join-Path $sourceRoot 'node_modules\rcedit\bin\rcedit-x64.exe'
$installerOutput = Join-Path $installerDirectory "软件管家-Setup-$Version.exe"

function Set-ExecutableIcon {
    param(
        [string]$Executable,
        [string]$Icon
    )

    if (-not (Test-Path -LiteralPath $rcedit)) { throw "未找到图标工具。请先在源码目录运行 npm install。" }
    & $rcedit $Executable --set-icon $Icon
    if ($LASTEXITCODE -ne 0) { throw '写入程序图标失败。' }
}

if (-not (Get-Command dnx -ErrorAction SilentlyContinue)) {
    throw '未找到 dnx，无法运行 Velopack。'
}
if (-not (Test-Path -LiteralPath $projectFile)) { throw "未找到项目配置：$projectFile" }
if (-not (Test-Path -LiteralPath $iconFile)) { throw "未找到程序图标：$iconFile" }

Write-Host '[1/4] 生成程序图标并构建桌面程序…'
python (Join-Path $sourceRoot 'scripts\generate-app-icon.py')
if ($LASTEXITCODE -ne 0) { throw '生成程序图标失败。' }

Push-Location $sourceRoot
try {
    npm run build:exe
    if ($LASTEXITCODE -ne 0) { throw '构建桌面程序失败。' }
} finally {
    Pop-Location
}

if (-not (Test-Path -LiteralPath $executableSource)) { throw "未找到构建结果：$executableSource" }
Set-ExecutableIcon -Executable $executableSource -Icon $iconFile

Write-Host '[2/4] 准备安装内容…'
Remove-Item -LiteralPath $temporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $packageDirectory, $releaseDirectory, $installerDirectory -Force | Out-Null
Copy-Item -LiteralPath $executableSource -Destination (Join-Path $packageDirectory 'software-manager.exe') -Force

$runtimeScripts = @('run-wegame-ocr.ps1', 'visual-target.ps1')
$runtimeDirectory = Join-Path $packageDirectory 'scripts'
New-Item -ItemType Directory -Path $runtimeDirectory -Force | Out-Null
foreach ($script in $runtimeScripts) {
    $scriptPath = Join-Path $PSScriptRoot $script
    if (-not (Test-Path -LiteralPath $scriptPath)) { throw "未找到运行辅助文件：$scriptPath" }
    Copy-Item -LiteralPath $scriptPath -Destination $runtimeDirectory -Force
}

Write-Host '[3/4] 生成 Velopack 安装程序…'
$vpkArguments = @(
    'vpk',
    '--version', '1.2.0',
    '--',
    'pack',
    # Velopack 默认使用 %LocalAppData%\<packId> 作为安装目录。
    # Tauri 的 identifier 也会使用 %LocalAppData%\com.wangjiboxi.softwaremanager
    # 保存应用数据；两者不能共用同一个目录。
    '--packId', 'com.wangjiboxi.softwaremanager.velopack',
    '--packVersion', $Version,
    '--packDir', $packageDirectory,
    '--mainExe', 'software-manager.exe',
    '--packAuthors', 'boxi-wangji',
    '--packTitle', '软件管家',
    '--icon', $iconFile,
    '--outputDir', $releaseDirectory,
    '--noPortable', 'true'
)
& dnx @vpkArguments
if ($LASTEXITCODE -ne 0) { throw 'Velopack 安装包构建失败。' }

$setup = Get-ChildItem -LiteralPath $releaseDirectory -Filter '*-Setup.exe' -File -Recurse | Select-Object -First 1
if ($null -eq $setup) { throw 'Velopack 未生成安装程序。' }
Copy-Item -LiteralPath $setup.FullName -Destination $installerOutput -Force

Write-Host '[4/4] 清理临时文件…'
Remove-Item -LiteralPath $temporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $env:CARGO_TARGET_DIR -Recurse -Force -ErrorAction SilentlyContinue
Write-Host "完成：$installerOutput"
Get-FileHash -LiteralPath $installerOutput -Algorithm SHA256 | Format-List
