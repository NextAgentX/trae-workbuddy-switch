<#
.SYNOPSIS
  BuddySwitch 一键打包 —— Windows NSIS 安装包（.exe）。

.DESCRIPTION
  一条命令走完「打版本戳 → 编译 → 打包 → 收集产物」全流程，产物落到 deliverables/。
  纯 PowerShell + Node 实现，**不依赖 bash/sh**（本机没有 Git Bash，`npm run bundle:nsis`
  那类 `sh scripts/*.sh` 的条目在本机跑不了）。

  自动处理的事项：
    - 清掉进程环境里重复的大小写代理变量（http_proxy 与 HTTP_PROXY 同时存在会让 .NET
      构造子进程环境块时抛「字典中的关键字重复」→ 任何原生命令都起不来，而报错完全看不出原因）；
    - 自动定位受管 Node 与 rustup 工具链（cargo 不在 PATH 也能用）；
    - 检测 ~/.buddy-switch/buddy-switch-updater.key：有则签名生成 updater 产物（.sig），
      无则临时关掉 createUpdaterArtifacts，只打本地安装包、不报错；
    - 构建输出同时落 logs/，便于失败后复查；
    - 校验产物**确实是本次构建生成的**（时间戳新鲜度），避免构建失败却把旧 exe 当成功。

.PARAMETER Mode
  release（默认）或 debug。

.PARAMETER SignPassword
  updater 私钥密码，默认 buddy-switch-dev。

.PARAMETER OutDir
  产物收集目录，默认 deliverables（相对仓库根）。

.PARAMETER SkipStamp
  跳过版本戳写入，沿用 package.json 里的现有版本（调试构建流程时用）。

.PARAMETER Fast
  快速模式：前端只跑 `vite build`，跳过 `tsc` 全量类型检查与 `npm run check:api`
  （前端 API 契约门禁）。省下的时间都在前端，Rust 侧照常编译。
  ⚠️ 改动过 Tauri 命令 / api.ts 路由时不要用 —— 契约门禁正是靠 check:api 兜底的，
  跳过后「前端调用与后端命令对不上」这类问题会一路带进安装包里。

.EXAMPLE
  .\scripts\package-windows.ps1
  .\scripts\package-windows.ps1 -Mode debug
  .\scripts\package-windows.ps1 -SkipStamp
  .\scripts\package-windows.ps1 -Fast

.NOTES
  也可以直接双击仓库根目录的 build-exe.cmd。
#>
[CmdletBinding()]
param(
  [ValidateSet("release", "debug")]
  [string]$Mode = "release",
  [string]$SignPassword = "buddy-switch-dev",
  [string]$OutDir = "deliverables",
  [switch]$SkipStamp,
  [switch]$Fast
)

$ErrorActionPreference = "Stop"

function Write-Step {
  param([string]$No, [string]$Msg, [string]$Color = "Cyan")
  Write-Host ("[{0}] {1}" -f $No, $Msg) -ForegroundColor $Color
}

function Fail {
  param([string]$Msg)
  Write-Host ""
  Write-Host "[FAILED] $Msg" -ForegroundColor Red
  exit 1
}

# node / cargo 的输出是 UTF-8，而 Windows 控制台默认按 ANSI(936) 解码 →
# 一旦被上层 `| Out-File` 或重定向捕获，中文会变成「宸叉洿鏂?」这类乱码。
# 只影响可读性、不影响退出码，但排查构建失败时非常碍事。
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch { }

$started = Get-Date
$Root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
Set-Location -LiteralPath $Root

Write-Host ""
Write-Host "BuddySwitch Windows 打包" -ForegroundColor Green
Write-Host ("仓库: {0}" -f $Root) -ForegroundColor DarkGray
Write-Host ("模式: {0}" -f $Mode) -ForegroundColor DarkGray
Write-Host ""

# ---------------------------------------------------------------- 0. 环境预置
# ⚠️ 本机进程环境块里同时存在 http_proxy 与 HTTP_PROXY（大小写各一份）。
# .NET 依据环境块构造子进程环境时按**不区分大小写**去重，撞键会抛
# 「字典中的关键字重复」→ 原生命令（node/cargo/cmd）全都起不来，而报错信息里
# 一个字都不会提到代理，极易误判成代码或工具链问题。
# 只清掉 **Process 作用域的小写副本**，大写那份仍在，代理照常可用。
foreach ($name in @("http_proxy", "https_proxy")) {
  $lowerVal = [System.Environment]::GetEnvironmentVariable($name, "Process")
  $upperName = $name.ToUpperInvariant()
  $upperVal = [System.Environment]::GetEnvironmentVariable($upperName, "Process")
  if ($lowerVal -and $upperVal) {
    [System.Environment]::SetEnvironmentVariable($name, $null, "Process")
    Write-Step "env" ("清除重复的 {0}（保留 {1}）以避免子进程环境块撞键" -f $name, $upperName) "DarkYellow"
  }
}

# ---------------------------------------------------------------- 1. 工具链
Write-Step "1/5" "解析工具链 ..."

$nodeExe = $null
$nodeCmd = Get-Command node.exe -ErrorAction SilentlyContinue
if ($nodeCmd) { $nodeExe = $nodeCmd.Source }
if (-not $nodeExe) {
  $versionsDir = Join-Path $env:USERPROFILE ".workbuddy\binaries\node\versions"
  if ([System.IO.Directory]::Exists($versionsDir)) {
    $cand = Get-ChildItem -LiteralPath $versionsDir -Filter "node.exe" -Recurse -ErrorAction SilentlyContinue |
      Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($cand) { $nodeExe = $cand.FullName }
  }
}
if (-not $nodeExe) { Fail "未找到 node.exe，请安装 Node 或把受管 Node 放到 ~/.workbuddy/binaries/node/versions/ 下。" }

$nodeDir = Split-Path -Parent $nodeExe
if (-not (($env:PATH -split ";") -contains $nodeDir)) { $env:PATH = "$nodeDir;$env:PATH" }

# tauri 的 beforeBuildCommand 是 `npm run build`，由 CLI 通过 cmd 启动 → npm.cmd 必须在 PATH 上。
$npmCmd = Join-Path $nodeDir "npm.cmd"
if (-not [System.IO.File]::Exists($npmCmd)) {
  $npmWhere = Get-Command npm.cmd -ErrorAction SilentlyContinue
  if ($npmWhere) { $npmCmd = $npmWhere.Source }
}
if (-not [System.IO.File]::Exists($npmCmd)) { Fail "未找到 npm.cmd（tauri 需要它执行 `npm run build`）。" }

$cargoExe = $null
$cargoCmd = Get-Command cargo.exe -ErrorAction SilentlyContinue
if ($cargoCmd) { $cargoExe = $cargoCmd.Source }
$cargoDirs = @((Join-Path $env:USERPROFILE ".cargo\bin"))
$toolchains = Join-Path $env:USERPROFILE ".rustup\toolchains"
if ([System.IO.Directory]::Exists($toolchains)) {
  $ds = Get-ChildItem -LiteralPath $toolchains -Directory -ErrorAction SilentlyContinue
  foreach ($d in $ds) { $cargoDirs += (Join-Path $d.FullName "bin") }
}
if (-not $cargoExe) {
  foreach ($d in $cargoDirs) {
    $p = Join-Path $d "cargo.exe"
    if ([System.IO.File]::Exists($p)) { $cargoExe = $p; break }
  }
}
if (-not $cargoExe) { Fail "未找到 cargo.exe，请先安装 Rust 工具链 (https://rustup.rs)。" }

# ⚠️ 必须把工具链目录**加进 PATH**，不能只用全路径调 cargo：
# src-tauri 的 build script（embed-resource）会 spawn `rustc` 走 PATH，
# 找不到就 panic `couldn't get rust version`（exit 101），看起来像代码坏了。
foreach ($d in @((Split-Path -Parent $cargoExe)) + $cargoDirs) {
  if ($d -and ([System.IO.Directory]::Exists($d)) -and (-not (($env:PATH -split ";") -contains $d))) {
    $env:PATH = "$d;$env:PATH"
  }
}

$tauriJs = Join-Path $Root "node_modules\@tauri-apps\cli\tauri.js"
if (-not [System.IO.File]::Exists($tauriJs)) {
  Fail "缺少 node_modules\@tauri-apps\cli\tauri.js，请先在仓库根执行 `npm install`。"
}

$nodeVer = ((& $nodeExe --version 2>&1) | Out-String).Trim()
$cargoVer = ((& $cargoExe --version 2>&1) | Out-String).Trim()
Write-Host ("      node  : {0} ({1})" -f $nodeVer, $nodeExe) -ForegroundColor DarkGray
Write-Host ("      cargo : {0} ({1})" -f $cargoVer, $cargoExe) -ForegroundColor DarkGray
Write-Host ("      npm   : {0}" -f $npmCmd) -ForegroundColor DarkGray

# ---------------------------------------------------------------- 2. 版本戳
# 版本号在**打包时从系统时间推导**（`<YYYY>.<M>.<DHHMM>`），不手工 bump，详见
# scripts/stamp-version.mjs。必须早于 tauri build：tauri 从 package.json /
# tauri.conf.json 取版本，而 4 个 crate 的 `env!("CARGO_PKG_VERSION")` 在编译期嵌入，
# 漏掉任何一处都会出现「安装包版本 ≠ API 服务页显示版本」。
Write-Step "2/5" "写入打包版本号 ..."
$stampScript = Join-Path $PSScriptRoot "stamp-version.mjs"
if (-not [System.IO.File]::Exists($stampScript)) { Fail "未找到 $stampScript" }

if ($SkipStamp) {
  Write-Host "      -SkipStamp：沿用现有版本号。" -ForegroundColor Yellow
} else {
  & $nodeExe $stampScript
  if ($LASTEXITCODE -ne 0) { Fail "版本戳写入失败 (exit $LASTEXITCODE)。" }
  # 校验所有载体一致 —— 漏改一处就是「安装包叫 A、界面显示 B」那类难查的问题。
  & $nodeExe $stampScript --check
  if ($LASTEXITCODE -ne 0) { Fail "版本载体校验不通过（存在漏改的版本号）。" }
}
$stampedVer = [string]((Get-Content -LiteralPath (Join-Path $Root "package.json") -Raw | ConvertFrom-Json).version)
Write-Host ("      本次打包版本：{0}" -f $stampedVer) -ForegroundColor Green

# ---------------------------------------------------------------- 3. 签名密钥
Write-Step "3/5" "检测 updater 签名密钥 ..."
$keyPath = Join-Path $env:USERPROFILE ".buddy-switch\buddy-switch-updater.key"
$tempCfg = $null
$signing = $false
# 需要临时改写的 tauri 配置项（bundle / build …）先攒在这里，最后合并成**一个** --config 文件：
# tauri 只接受最后一份 --config，拆成多个文件会互相覆盖。
$cfgOverrides = [ordered]@{}

# 用「能否读到内容」判断，而不是 Test-Path / FileInfo.Exists ——
# 本工作区的元数据探测偶发假报 False（见 .workbuddy/memory/MEMORY-2-tooling.md）。
$keyText = $null
try {
  $keyText = (Get-Content -LiteralPath $keyPath -Raw -ErrorAction Stop).Trim()
} catch {
  $keyText = $null
}

if ($keyText) {
  $env:TAURI_SIGNING_PRIVATE_KEY = $keyText
  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $SignPassword
  $signing = $true
  Write-Host "      检测到私钥，将生成 updater 签名产物（.sig）。" -ForegroundColor Green
} else {
  # 缺密钥时若 createUpdaterArtifacts=true，tauri 会直接构建失败 → 用临时配置关掉它。
  Write-Host ("      未找到 {0}，仅打本地安装包（跳过 updater 签名）。" -f $keyPath) -ForegroundColor Yellow
  $cfgOverrides["bundle"] = @{ createUpdaterArtifacts = $false }
}

# -Fast：把 beforeBuildCommand 换成 `npm run build:fast`（只跑 vite build）。
# 仍会完整重建前端 —— 只是不做类型检查与契约门禁，dist 一定是新的。
if ($Fast) {
  Write-Host "      -Fast：跳过 tsc 类型检查与 check:api（改过 Tauri 命令时不要用）。" -ForegroundColor Yellow
  $cfgOverrides["build"] = @{ beforeBuildCommand = "npm run build:fast" }
}

if ($cfgOverrides.Count -gt 0) {
  $tempCfg = Join-Path $env:TEMP ("buddy-switch-build-cfg-{0}.json" -f $PID)
  # 显式 **无 BOM** 的 UTF-8：`Set-Content -Encoding UTF8` 在 PS 5.1 会带 BOM，
  # tauri 解析配置首字节就会撞上，报错信息完全看不出是编码问题。
  [System.IO.File]::WriteAllText(
    $tempCfg,
    ($cfgOverrides | ConvertTo-Json -Depth 5),
    (New-Object System.Text.UTF8Encoding($false))
  )
}

# ---------------------------------------------------------------- 4. 构建
Write-Step "4/5" ("开始 tauri build ({0}, nsis, v{1}) ..." -f $Mode, $stampedVer)

$logDir = Join-Path $Root "logs"
if (-not [System.IO.Directory]::Exists($logDir)) { New-Item -ItemType Directory -Path $logDir | Out-Null }
$logPath = Join-Path $logDir ("build-{0}-{1}.log" -f $Mode, $stampedVer)
Write-Host ("      日志: {0}" -f $logPath) -ForegroundColor DarkGray

$tauriArgLine = @('build', '--bundles', 'nsis', '--ci')
if ($Mode -eq "debug") { $tauriArgLine += '--debug' }
if ($tempCfg) { $tauriArgLine += '--config'; $tauriArgLine += $tempCfg }

# 用临时 .cmd 承载命令行：绕开 PowerShell 5.1 向原生命令传参时的引号转义问题，
# 同时拿到**实时**日志（`| Out-File` 要等构建结束才落盘，中途挂住看不出进度）。
$batPath = Join-Path $env:TEMP ("buddy-switch-build-{0}.cmd" -f $PID)

function New-BuildBat {
  param([string]$Path, [string]$Redirect)
  $batQuoted = ($tauriArgLine | ForEach-Object { '"' + $_ + '"' }) -join ' '
  $batLines = @(
    '@echo off',
    'chcp 65001 >nul',
    ('"{0}" "{1}" {2} {3} "{4}" 2>&1' -f $nodeExe, $tauriJs, $batQuoted, $Redirect, $logPath),
    'exit /b %ERRORLEVEL%'
  )
  Set-Content -LiteralPath $Path -Encoding ASCII -Value $batLines
}

New-BuildBat -Path $batPath -Redirect '>'
& cmd.exe /c $batPath
$buildExit = $LASTEXITCODE

# ⚠️ 已知偶发失败（本机特有，**与项目代码无关**，出现时别去改代码）：
# 本机工具链的 sysroot 把 core / std 的 rlib 做成了「元数据存根」形态
# （`libcore.rlib` 仅 2.3MB，完整元数据在 63MB 的 `libcore.rmeta` 里 —— 即 Rust 侧
# `-Zembed-metadata=no` 实验的产物形态）。个别 rustc 进程偶发加载不到对应的 .rmeta，就报
#     error: only metadata stub found for `rlib` dependency `core`
# 并连带刷出一片 `cannot resolve a prelude import` / `cannot find trait Into`
# （症状出现在 dpi / block-buffer 这类 `#![no_std]` crate 上，全是下游假象）。
# 实测：同一份代码重跑即通过，且 cargo 续用第一次的缓存，第二次快得多。
# 因此**只在这个特征串出现时自动重试一次**；其他编译错误照常立即失败，不掩盖真问题。
if ($buildExit -ne 0) {
  $knownFlake = Select-String -LiteralPath $logPath -Pattern 'only metadata stub found for' -SimpleMatch -Quiet -ErrorAction SilentlyContinue
  if ($knownFlake) {
    Write-Host "[warn] 命中已知偶发的「元数据存根」错误（工具链 sysroot 形态导致，非代码问题），自动重试一次 ..." -ForegroundColor Yellow
    New-BuildBat -Path $batPath -Redirect '>>'
    & cmd.exe /c $batPath
    $buildExit = $LASTEXITCODE
  }
}

[System.IO.File]::Delete($batPath)
if ($tempCfg) { [System.IO.File]::Delete($tempCfg) }

if ($buildExit -ne 0) {
  Write-Host ""
  Write-Host "构建失败，日志尾部：" -ForegroundColor Red
  $tail = Get-Content -LiteralPath $logPath -Tail 40 -ErrorAction SilentlyContinue
  foreach ($line in $tail) { Write-Host ("  {0}" -f $line) -ForegroundColor DarkGray }
  if ($null -eq $buildExit) {
    Fail "未能启动构建进程（未取得退出码，不是编译错误）。"
  }
  Fail ("tauri build 失败 (exit {0})，完整日志：{1}" -f $buildExit, $logPath)
}

# ---------------------------------------------------------------- 5. 收集产物
Write-Step "5/5" "收集产物 ..."

# src-tauri 属于 workspace 成员，cargo 的 target 目录在**仓库根**；
# 保留 src-tauri\target 分支以兼容独立构建（非 workspace）的情况。
$searchDirs = @(
  (Join-Path $Root ("src-tauri\target\{0}\bundle\nsis" -f $Mode)),
  (Join-Path $Root ("target\{0}\bundle\nsis" -f $Mode))
)

$exe = $null
$exeDir = $null
foreach ($d in $searchDirs) {
  $found = Get-ChildItem -LiteralPath $d -Filter "*.exe" -File -ErrorAction SilentlyContinue |
    Where-Object { $_.LastWriteTime -ge $started.AddMinutes(-1) } |
    Sort-Object LastWriteTime -Descending
  if ($found) {
    $exe = $found | Select-Object -First 1
    $exeDir = $d
    break
  }
}
if (-not $exe) {
  $all = @()
  foreach ($d in $searchDirs) {
    $all += Get-ChildItem -LiteralPath $d -Filter "*.exe" -File -ErrorAction SilentlyContinue
  }
  if ($all.Count -gt 0) {
    Fail ("构建目录里只有**旧**安装包（时间戳早于本次构建），说明本次没有产出新包。请查看日志：{0}" -f $logPath)
  }
  Fail ("未找到安装包。已查找：{0}" -f ($searchDirs -join " | "))
}

$destDir = Join-Path $Root $OutDir
if (-not [System.IO.Directory]::Exists($destDir)) { New-Item -ItemType Directory -Path $destDir | Out-Null }

$destExe = Join-Path $destDir $exe.Name
Copy-Item -LiteralPath $exe.FullName -Destination $destExe -Force

# .sig 必须跟安装包一起发布：客户端要拿它做 minisign 验签，缺了自动更新会在
# 「签名缺失/不匹配」处失败，而安装包本身看起来完全正常。
$sigPath = "$($exe.FullName).sig"
$destSig = "$destExe.sig"
$haveSig = $false
if ([System.IO.File]::Exists($sigPath)) {
  Copy-Item -LiteralPath $sigPath -Destination $destSig -Force
  $haveSig = $true
}

$sizeMb = [math]::Round($exe.Length / 1MB, 2)
Write-Host ""
Write-Host "安装包 : $destExe" -ForegroundColor Green
Write-Host ("大小   : {0} MB" -f $sizeMb) -ForegroundColor Green
if ($haveSig) {
  Write-Host "签名   : $destSig" -ForegroundColor Green
} elseif ($signing) {
  Write-Host "[warn] 已启用 updater 签名，但未找到 .sig —— 发布前请确认签名步骤是否生效。" -ForegroundColor Red
} else {
  Write-Host "[warn] 未启用 updater 签名，本次没有 .sig（本地安装包可用，不能用于自动更新）。" -ForegroundColor Yellow
}
Write-Host ("构建目录: {0}" -f $exeDir) -ForegroundColor DarkGray
Write-Host ("日志    : {0}" -f $logPath) -ForegroundColor DarkGray
Write-Host ("耗时    : {0:N1} 分钟" -f ((Get-Date) - $started).TotalMinutes) -ForegroundColor DarkGray
Write-Host ""
Write-Host ("[done] 打包完成 (v{0}, {1})" -f $stampedVer, $Mode) -ForegroundColor Green

if ($signing -and (-not $haveSig)) { exit 1 }
exit 0
