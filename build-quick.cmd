@echo off
REM 本文件必须按 **GBK(936)** 保存 —— cmd 用本机 ANSI 代码页解析 .cmd 的字节，
REM 存成 UTF-8 会把中文注释切成乱码并当成命令执行（实测报「不是内部或外部命令」）。
REM 控制台编码不必管：被调用的 package-windows.ps1 会自己把控制台切到 UTF-8，
REM 退出后回到 936，两边的中文都能正常显示。
setlocal enabledelayedexpansion
REM ============================================================================
REM  BuddySwitch 快速打包（自测用）—— 双击即可出一个 NSIS 安装包
REM
REM  正式发布请用 build-exe.cmd；这个脚本面向「我要赶紧装一个试试」：
REM    - 默认跳过 tsc 全量类型检查与前端 API 契约门禁（只跑 vite build），
REM      Rust 侧照常完整编译、照常打版本戳与 updater 签名；
REM    - 打包前自动结束正在运行的客户端（它锁着 target 里的 exe，否则链接阶段
REM      报 LNK1104，看起来像代码错误，其实是文件被占用）；
REM    - 打完自动打开 deliverables 并选中新安装包。
REM
REM  用法： build-quick.cmd [fast^|full] [release^|debug] [nostamp] [noopen] [nopause] [kill]
REM
REM    fast      （默认）跳过 tsc + check:api
REM    full       完整前端校验，等价 build-exe.cmd —— 改过 Tauri 命令 / api.ts
REM               路由 / 前端接口时用这个，否则契约门禁被跳过后问题会带进安装包
REM    debug      debug 构建，编译快很多（只适合本机跑起来看，别拿它验证自动更新）
REM    nostamp    不写版本戳，沿用 package.json 现有版本 —— 同日重打会同名覆盖，
REM               客户端还会误判「已是最新」而跳过更新，测自动更新时千万别加
REM    noopen     打完不打开 deliverables
REM     nopause    打完不暂停（被别的脚本调用时用）
REM     kill       打包前强制结束正在运行的客户端（被锁导致 LNK1104 时用）
REM
REM  例： build-quick.cmd              （最常见的「改了前端，打个包装一遍」）
REM        build-quick.cmd full        （改了 Tauri 命令，要过一遍契约门禁）
REM        build-quick.cmd debug       （只想最快跑起来看一眼）
REM ============================================================================

cd /d "%~dp0"

set "FAST=1"
set "MODE="
set "DOOPEN=1"
set "DOPAUSE=1"
set "KILL=0"
set "STAMP=1"

:parse
if "%~1"=="" goto :run
if /i "%~1"=="help"    goto :usage
if /i "%~1"=="/?"      goto :usage
if /i "%~1"=="fast"    (set "FAST=1"      & shift & goto :parse)
if /i "%~1"=="full"    (set "FAST=0"      & shift & goto :parse)
if /i "%~1"=="release" (set "MODE=release" & shift & goto :parse)
if /i "%~1"=="debug"   (set "MODE=debug"   & shift & goto :parse)
if /i "%~1"=="nostamp" (set "STAMP=0"     & shift & goto :parse)
if /i "%~1"=="noopen"  (set "DOOPEN=0"    & shift & goto :parse)
if /i "%~1"=="nopause" (set "DOPAUSE=0"   & shift & goto :parse)
if /i "%~1"=="kill"    (set "KILL=1"      & shift & goto :parse)
echo [warn] 忽略未识别的参数：%~1
shift
goto :parse

:usage
echo.
echo 用法： build-quick.cmd [fast^|full] [release^|debug] [nostamp] [noopen] [nopause] [kill]
echo.
echo   fast    （默认）跳过 tsc + check:api
echo   full     完整前端校验（改过 Tauri 命令 / 接口时用）
echo   debug    debug 构建，编译快（只适合本机看，不用于验证自动更新）
echo   nostamp  不打版本戳（同日重打会同名覆盖，测自动更新别加）
echo   noopen   打完不打开 deliverables
echo   nopause  打完不暂停（被别的脚本调用时用）
echo   kill     打包前强制结束正在运行的客户端（被锁导致 LNK1104 时用）
echo.
exit /b 0

:run
echo.
echo === BuddySwitch 快速打包 ===
echo.

REM 正在运行的客户端会锁住 target\release\buddy-switch*.exe，链接阶段会报 LNK1104。
REM 那是锁冲突不是代码错误，所以这里直接结束进程，不再让用户手动找。
for %%P in ("buddy-switch.exe" "buddy-switch-rust.exe") do (
  tasklist /fi "imagename eq %%~P" 2>nul | findstr /i /c:"%%~P" >nul
  if not errorlevel 1 (
    if "%KILL%"=="1" (
      echo [warn] %%~P 正在运行，按 /kill 结束它。
      taskkill /im %%~P /f >nul 2>&1
    ) else (
      echo [note] %%~P 正在运行（通常装在安装目录，并不锁 target\release）；
      echo       若链接阶段报 LNK1104，加 kill 参数或手动关掉客户端。
    )
  )
)

set "PSARGS="
if "%FAST%"=="1"  set "PSARGS=!PSARGS! -Fast"
if "%STAMP%"=="0" set "PSARGS=!PSARGS! -SkipStamp"
if defined MODE   set "PSARGS=!PSARGS! -Mode %MODE%"

echo 参数：%PSARGS%（空表示全默认：快速模式 + release + 打版本戳）
echo.

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\package-windows.ps1" %PSARGS%
set "RC=%ERRORLEVEL%"

echo.
if not "%RC%"=="0" (
  echo [FAILED] 打包失败（exit %RC%）。看上面输出，或 logs\build-*.log。
) else (
  echo [OK] 打包完成。
)

if "%RC%"=="0" if "%DOOPEN%"=="1" (
  set "LATEST="
  for /f "delims=" %%F in ('dir /b /o-d "%~dp0deliverables\*_x64-setup.exe" 2^>nul') do (
    if not defined LATEST set "LATEST=%%F"
  )
  if defined LATEST (
    echo 安装包：%~dp0deliverables\!LATEST!
    start "" explorer /select,"%~dp0deliverables\!LATEST!"
  ) else (
    echo [warn] deliverables 里没找到安装包。
  )
)

REM 仅在**双击启动**时暂停（被别的脚本调用时不阻塞）
echo %CMDCMDLINE% | findstr /i /c:"%~nx0" >nul
if not errorlevel 1 if "%DOPAUSE%"=="1" pause

exit /b %RC%
