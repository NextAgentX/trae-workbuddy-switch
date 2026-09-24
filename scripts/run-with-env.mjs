#!/usr/bin/env node
/**
 * 跨平台地「设置环境变量后执行命令」——替代 POSIX 内联写法 `FOO=1 cmd`。
 *
 * ## 为什么需要它（2026-09-24 实测踩到）
 *
 * `package.json` 里原本写的是：
 *
 * ```jsonc
 * "build:demo": "tsc && VITE_DEMO_MODE=1 VITE_PAGES_DEMO=1 vite build --outDir dist-demo"
 * ```
 *
 * 这在 Git Bash / macOS / Linux 下没问题，但 **`npm run` 在 Windows 上走 `cmd.exe`**，
 * `VITE_DEMO_MODE=1` 会被当成一条命令去执行，报：
 *
 * ```text
 * 'VITE_DEMO_MODE' 不是内部或外部命令，也不是可运行的程序或批处理文件。
 * ```
 *
 * ⇒ `build:demo` / `dev:demo` / `tauri:dev:screenshot` 三条脚本在 Windows 上**直接不可用**。
 * 这类缺陷的坏处是「看起来只是脚本问题」，但它挡住的恰好是**演示构建**这条
 * 出截图 / 出文档的路径，而 CI 与日常 `npm run build` 都是绿的 ⇒ 没人会发现。
 *
 * ## 为什么不引 `cross-env`
 *
 * 本项目 devDependencies 极简（只有 vite / tauri / tailwind / typescript / 类型包），
 * 为一条脚本引入一个运行时依赖不划算；而这件事用 Node 标准库 30 行就能做对。
 *
 * ## 用法
 *
 * ```text
 * node scripts/run-with-env.mjs FOO=1 BAR=2 -- vite build --outDir dist-demo
 * ```
 *
 * - `--` 之前一律按 `KEY=VALUE` 解析（`KEY=` 表示置空字符串，而不是删除变量）；
 * - `--` 之后是**要执行的命令及其参数**，逐字透传；
 * - 退出码 = 子进程退出码；被信号杀死时按 `128 + signal` 返回（与 shell 惯例一致）。
 *
 * ## Windows 上必须 `shell: true`（不是可选项）+ **必须自己给参数加引号**
 *
 * `node_modules/.bin` 里的 `vite` / `tauri` 在 Windows 上是 `.cmd` 垫片，
 * 而 `child_process.spawn` **不带 shell 时不会解析 `.cmd`** ⇒ 会报 `ENOENT`。
 * npm 已经把 `.bin` 加进 PATH，所以带 shell 后能正常命中。
 *
 * ⚠️ 但 `shell: true` 有个官方明示的坑：**Node 只把参数用空格拼起来，不做任何转义**
 * （`child_process` 文档 "shell" 一节）⇒ 参数里带空格 / `&` / `^` 等会被 cmd 重新解释。
 * 所以下面用 {@link quoteForCmd} 自己补引号。**踩到过**：阳性对照里写
 * `node -e "console.log(process.env.FOO)"`，未加引号保护时 cmd 把表达式切碎，
 * 报 `SyntaxError: Unexpected end of input`。
 */

import { spawn } from "node:child_process";

/** 解析 `KEY=VALUE ... -- cmd args...`。 */
function parseArgv(argv) {
  const separator = argv.indexOf("--");
  if (separator < 0) {
    throw new Error("缺少 `--` 分隔符：请写成 `node scripts/run-with-env.mjs KEY=VALUE -- <命令> [参数…]`");
  }
  const assignments = argv.slice(0, separator);
  const command = argv.slice(separator + 1);
  if (command.length === 0) {
    throw new Error("`--` 之后没有要执行的命令");
  }

  const env = {};
  for (const assignment of assignments) {
    const eq = assignment.indexOf("=");
    if (eq <= 0) {
      throw new Error(`环境变量必须是 KEY=VALUE 形式，收到: ${JSON.stringify(assignment)}`);
    }
    env[assignment.slice(0, eq)] = assignment.slice(eq + 1);
  }
  return { env, command: command[0], args: command.slice(1) };
}

/**
 * 按 `cmd.exe` 的规则给单个参数加引号。
 *
 * 只处理**本项目会遇到**的形态：空串、含空白、含 `cmd` 元字符。
 * `%` 刻意不处理（`cmd` 会在引号内也展开它，要真正转义得写 `%%`，
 * 而本项目的参数里没有 `%`；写一条用不到的规则只会误导后来者）。
 */
function quoteForCmd(arg) {
  if (arg === "") return '""';
  // 简单参数原样返回：给每个参数都套引号会让人以为存在转义问题。
  if (!/[\s"&|<>^()]/.test(arg)) return arg;
  // 带引号参数内部的 `"` 在 cmd 里用 `""` 表示。
  return `"${arg.replace(/"/g, '""')}"`;
}

function main() {
  const { env, command, args } = parseArgv(process.argv.slice(2));

  const useShell = process.platform === "win32";
  const child = spawn(
    useShell ? [command, ...args].map(quoteForCmd).join(" ") : command,
    useShell ? [] : args,
    {
      stdio: "inherit",
      // 只叠加、不替换：`PATH` / `APPDATA` 这类父进程环境必须原样继承。
      env: { ...process.env, ...env },
      // Windows：`.cmd` 垫片必须经 shell 解析（见文件头说明）。
      shell: useShell,
    },
  );

  child.on("error", (error) => {
    process.stderr.write(`无法执行 ${command}: ${error.message}\n`);
    process.exit(1);
  });
  child.on("exit", (code, signal) => {
    if (signal) {
      process.stderr.write(`${command} 被信号 ${signal} 终止\n`);
      // 只映射两个最常被 CI / 用户中断触发的信号；其余按 1（失败）返回。
      // 不做「128 + signal 编号」—— Node 给的是名字（`SIGINT`）而不是编号，
      // 硬凑一张表只会多一处要维护的映射。
      process.exit(signal === "SIGINT" ? 130 : signal === "SIGTERM" ? 143 : 1);
    }
    process.exit(code ?? 0);
  });
}

try {
  main();
} catch (error) {
  process.stderr.write(`run-with-env 无法执行：${error && error.message ? error.message : error}\n`);
  process.exit(1);
}
