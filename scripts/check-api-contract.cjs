#!/usr/bin/env node
"use strict";

// API 契约 / 演示数据一致性护栏（零依赖，仅用 Node 标准库）。
//
// 本项目的双通道适配层有若干处「纯字符串」必须逐字对齐，且全部没有编译期保护：
//   ① src/lib/api.ts                       —— call("<cmd>") 的 cmd 字符串
//   ② src/lib/api.ts                       —— ROUTES 表的键名与 { method, path }
//   ③ crates/buddy-switch-server/src/api.rs   —— 路由表的 path + method（跨语言，编译器无法互校）
//   ④ src-tauri/src/lib.rs                 —— invoke_handler 登记的命令名
//   ⑤ src/lib/screenshot-demo.ts           —— screenshotDemoResponse 的 switch case 分支
//                                             （须覆盖 DEMO_READ_COMMANDS 的每个只读命令）
//
// 任一处漂移，症状都是「构建全绿，但某端静默失效」（桌面端 command not found /
// webui 抛「暂不支持」/ method 写反打到错的 handler / 演示模式运行时报缺只读数据）。
// 本脚本在构建前做这些一致性校验，发现差异立即以非零退出码失败，并打印具体差异。
//
// 有意不做的一项：webui POST body 形状（`{ config: … }` vs 扁平）。后端对二者都接受
// （`body.get("config").unwrap_or(&body)`），且 body 形状在源码里没有机器可读的声明，
// 无法用稳健的静态规则校验——强行用脆弱正则只会降低整条护栏的可信度，故不做。
//
//   ⑥ src-tauri/src/commands.rs  —— 桌面端命令的**参数名**（见第 7 条校验）
//
// 第 7 条只覆盖一个**可机检**的子集：签名里除 Tauri 注入参数外**只剩一个 `Value` 参数**
// 的命令。Tauri 按参数名从 invoke 载荷里取值，前端平铺传参就会报
// `missing required key <参数名>`（真踩过：`trae_switch_account` 平铺传 `userId/launch/…`，
// 报 `missing required key options`，而 webui 通道因为 `body.get("options").unwrap_or(body)`
// 能容忍平铺 —— 于是「浏览器里好用、桌面端点不动」）。这类命令的参数名与形状是**单一、
// 机器可读**的，因此可以硬校验；其余多参数命令仍不做（那才需要脆弱的正则）。
//
// 用法：node scripts/check-api-contract.cjs   （npm run check:api）

const fs = require("fs");
const path = require("path");

const ROOT = path.resolve(__dirname, "..");
const API_TS = path.join(ROOT, "src", "lib", "api.ts");
const SCREENSHOT_DEMO_TS = path.join(ROOT, "src", "lib", "screenshot-demo.ts");
const SERVER_RS = path.join(ROOT, "crates", "buddy-switch-server", "src", "api.rs");
const TAURI_LIB_RS = path.join(ROOT, "src-tauri", "src", "lib.rs");
const TAURI_COMMANDS_RS = path.join(ROOT, "src-tauri", "src", "commands.rs");

// 第 7 条校验的豁免表。留空是**有意**的：目前所有「单 Value 参数」命令都能用对象
// 字面量调用，豁免项一旦出现，说明出现了需要人工判断的形态，应在评审时说明理由。
const SINGLE_VALUE_SHAPE_EXEMPT = new Set([]);

// webui 未提供、仅桌面端可用的命令。它们「有意」没有 ROUTES 路由条目——豁免的依据是
// 「无 ROUTES 路由」，而不是「没守卫」：这些命令的 wrapper **内部**都自带
// `demoModeEnabled` / `isWebui()` 早退守卫（见 api.ts 的 openPermissionSettings /
// checkAuthPermission / revealAppInFinder / relaunchApp / get|setLaunchAtLoginEnabled），
// 因此「webui 不可达」由函数自己保证，不依赖调用方自律。
// 除此之外的任何 call("<cmd>") 都必须有 ROUTES 条目、且必须已在桌面端登记。
const ROUTE_EXEMPT_COMMANDS = new Set([
  "open_permission_settings",
  "check_auth_permission",
  "reveal_app_in_finder",
  "relaunch_app",
  "get_launch_at_login_enabled",
  "set_launch_at_login_enabled",
]);

/** 读文件并去掉可能存在的 UTF-8 BOM。 */
function read(file) {
  return fs.readFileSync(file, "utf8").replace(/^\uFEFF/, "");
}

/**
 * 去掉行注释与块注释。
 *
 * 必要性（真实踩过）：注释里写 `call("<cmd>")` 这类示例会把扫描器骗过去，
 * 报出「未登记的命令 <cmd>」，而真正的调用点全都没问题——护栏于是从
 * 「保护构建」变成「制造噪声」，最后被开发者绕过。
 * 用 `(^|[^:])` 排除 `://`，避免把 URL 里的 `//` 当成行注释起点。
 */
function stripComments(text) {
  return text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:])\/\/[^\n]*/g, "$1");
}

/**
 * 从 openIndex（指向 "{"/"("/"["）开始，返回与之配对的括号**内部**文本。
 *
 * 只对同一种括号计数，足以覆盖本项目里的对象字面量与函数实参（不含嵌套异种括号）。
 */
function sliceBalanced(text, openIndex) {
  const open = text[openIndex];
  const close = open === "{" ? "}" : open === "(" ? ")" : open === "[" ? "]" : null;
  if (!close) throw new Error(`sliceBalanced: 起始字符不是括号 (${JSON.stringify(open)})`);
  let depth = 0;
  for (let i = openIndex; i < text.length; i += 1) {
    const ch = text[i];
    if (ch === open) {
      depth += 1;
    } else if (ch === close) {
      depth -= 1;
      if (depth === 0) return text.slice(openIndex + 1, i);
    }
  }
  throw new Error("sliceBalanced: 括号未闭合");
}

/**
 * `call` 调用点的匹配前缀：`call` + **可选的泛型实参** + `(` + 空白。
 *
 * 为什么放行泛型（真实踩过，2026-09-23）：`call<AppStatus>("get_status", …)` 是
 * 完全正常的 TS 写法 —— 返回类型无法从实参推断时（例如后面接
 * `.then(normalizeAppStatus)`）**必须**显式写出来。原来的 `call\s*\(` 不认泛型，
 * 于是这两条调用点直接扫不到，护栏反过来报「ROUTES["get_status"] 是死路由」。
 *
 * 这里的取舍：护栏的职责是「每个 `call` 的命令名都要有路由」，**不是**限制代码
 * 怎么写（检查脚本不得左右代码设计）。放行泛型不会削弱它 —— 只是让本该被扫到的
 * 调用点重新进入扫描范围；`[^(]*?` 顺带兼容 `Record<string, number>` 这类嵌套 `<>`。
 */
const CALL_PREFIX = String.raw`(?<![\w.$])call\s*(?:<[^(]*?>\s*)?\(\s*`;

/** 从 api.ts 的 ROUTES 表提取 { cmd, method, path } 列表。 */
function extractRoutes(apiTs) {
  const anchor = apiTs.indexOf("const ROUTES");
  if (anchor < 0) throw new Error("api.ts 中找不到 `const ROUTES`");
  const openIndex = apiTs.indexOf("{", anchor);
  if (openIndex < 0) throw new Error("api.ts 中找不到 ROUTES 对象字面量");
  const block = sliceBalanced(apiTs, openIndex);
  const re =
    /([A-Za-z0-9_]+)\s*:\s*\{\s*method\s*:\s*"([A-Za-z]+)"\s*,\s*path\s*:\s*"([^"]+)"\s*\}/g;
  const routes = [];
  let m;
  while ((m = re.exec(block)) !== null) {
    routes.push({ cmd: m[1], method: m[2].toUpperCase(), path: m[3] });
  }
  if (routes.length === 0) throw new Error("未能从 ROUTES 解析出任何路由条目");
  return routes;
}

/** 提取 api.ts 中所有裸 call("<cmd>") 的 cmd（排除 httpCall 等与函数定义）。 */
function extractCallCommands(apiTs) {
  // 先剥注释：注释中的示例调用不构成契约（见 stripComments 的说明）。
  const source = stripComments(apiTs);
  const re = new RegExp(`${CALL_PREFIX}"([^"]+)"`, "g");
  const cmds = new Set();
  let m;
  while ((m = re.exec(source)) !== null) cmds.add(m[1]);
  if (cmds.size === 0) throw new Error("未能从 api.ts 解析出任何 call(\"<cmd>\")");
  return cmds;
}

/** 提取 api.ts 中 DEMO_READ_COMMANDS 集合包含的只读命令名。 */
function extractDemoReadCommands(apiTs) {
  const anchor = apiTs.indexOf("DEMO_READ_COMMANDS");
  if (anchor < 0) throw new Error("api.ts 中找不到 DEMO_READ_COMMANDS");
  const openIndex = apiTs.indexOf("[", anchor);
  if (openIndex < 0) throw new Error("DEMO_READ_COMMANDS 不是数组字面量");
  const block = sliceBalanced(apiTs, openIndex);
  const cmds = new Set();
  const re = /"([^"]+)"/g;
  let m;
  while ((m = re.exec(block)) !== null) cmds.add(m[1]);
  if (cmds.size === 0) throw new Error("未能从 DEMO_READ_COMMANDS 解析出任何命令");
  return cmds;
}

/** 提取 screenshot-demo.ts 中 screenshotDemoResponse 的 switch case 分支命令名。 */
function extractDemoCaseCommands(screenshotTs) {
  const anchor = screenshotTs.indexOf("export function screenshotDemoResponse");
  if (anchor < 0) throw new Error("screenshot-demo.ts 中找不到 screenshotDemoResponse");
  const openIndex = screenshotTs.indexOf("{", anchor);
  if (openIndex < 0) throw new Error("screenshotDemoResponse 函数体未找到");
  const body = sliceBalanced(screenshotTs, openIndex);
  const cmds = new Set();
  const re = /case\s+"([^"]+)"/g;
  let m;
  while ((m = re.exec(body)) !== null) cmds.add(m[1]);
  if (cmds.size === 0) throw new Error("screenshotDemoResponse 中解析不到任何 case 分支");
  return cmds;
}

/** 从 server api.rs 提取 path → 支持的 method 集合。 */
function extractServerRoutes(serverRs) {
  const map = new Map();
  const routeRe = /\.route\s*\(/g;
  let m;
  while ((m = routeRe.exec(serverRs)) !== null) {
    const openIndex = m.index + m[0].length - 1;
    const inner = sliceBalanced(serverRs, openIndex);
    const pathMatch = inner.match(/"([^"]+)"/);
    if (!pathMatch) continue;
    const routePath = pathMatch[1];
    const methods = map.get(routePath) || new Set();
    const methodRe = /\b(get|post|put|patch|delete|head|options)\s*\(/g;
    let mm;
    while ((mm = methodRe.exec(inner)) !== null) methods.add(mm[1].toUpperCase());
    map.set(routePath, methods);
  }
  if (map.size === 0) throw new Error("未能从 server api.rs 解析出任何 .route(...)");
  return map;
}

/** 从 src-tauri lib.rs 的 generate_handler![...] 提取登记的命令名。 */
function extractInvokeCommands(libRs) {
  const block = libRs.match(/generate_handler!\s*\[([\s\S]*?)\]/);
  if (!block) throw new Error("lib.rs 中找不到 generate_handler![...]");
  const cmds = new Set();
  const re = /commands::([A-Za-z0-9_]+)/g;
  let m;
  while ((m = re.exec(block[1])) !== null) cmds.add(m[1]);
  if (cmds.size === 0) throw new Error("lib.rs invoke_handler 中解析不到命令名");
  return cmds;
}

/**
 * 按**顶层**逗号切分（跳过字符串、跳过 `{}`/`[]`/`()` 内部）。
 *
 * 用于解析 Rust 形参表与 TS 对象字面量的顶层条目——嵌套里的逗号不能当分隔符。
 */
function splitTopLevel(text) {
  const parts = [];
  let depth = 0;
  let current = "";
  let i = 0;
  while (i < text.length) {
    const ch = text[i];
    if (ch === '"' || ch === "'" || ch === "`") {
      const quote = ch;
      current += ch;
      i += 1;
      while (i < text.length) {
        current += text[i];
        if (text[i] === "\\") {
          current += text[i + 1] ?? "";
          i += 2;
          continue;
        }
        if (text[i] === quote) {
          i += 1;
          break;
        }
        i += 1;
      }
      continue;
    }
    if (ch === "{" || ch === "[" || ch === "(") depth += 1;
    if (ch === "}" || ch === "]" || ch === ")") depth -= 1;
    if (ch === "," && depth === 0) {
      parts.push(current);
      current = "";
      i += 1;
      continue;
    }
    current += ch;
    i += 1;
  }
  if (current.trim() !== "") parts.push(current);
  return parts;
}

/** 提取对象字面量内部的**顶层键名**（支持 `{ a: 1 }` 与 `{ a }` 简写）。 */
function topLevelKeys(objectBody) {
  return splitTopLevel(objectBody)
    .map((segment) => {
      const withValue = segment.match(/^\s*([A-Za-z_$][\w$]*)\s*:/);
      if (withValue) return withValue[1];
      const shorthand = segment.match(/^\s*([A-Za-z_$][\w$]*)\s*$/);
      return shorthand ? shorthand[1] : null;
    })
    .filter((key) => key !== null);
}

/** 把 snake_case 参数名转成 camelCase（`#[tauri::command(rename_all = "camelCase")]` 用）。 */
function toCamelCase(name) {
  return name.replace(/_([a-z0-9])/g, (_, ch) => ch.toUpperCase());
}

/**
 * 从 commands.rs 提取「除注入参数外只剩一个 `Value` 参数」的命令 → 期望的载荷键名。
 *
 * 返回 `Map<命令名, 期望键名>`。Tauri 注入的参数（`AppHandle` / `State` / `Window` …）
 * 由框架填充，不来自前端，必须先排除，否则每个带 `app` 的命令都会被算成多参数。
 */
function extractSingleValueParamCommands(commandsRs) {
  const source = stripComments(commandsRs);
  const map = new Map();
  const fnRe = /pub\s+(?:async\s+)?fn\s+([A-Za-z0-9_]+)\s*\(/g;
  let m;
  while ((m = fnRe.exec(source)) !== null) {
    const name = m[1];
    const openIndex = m.index + m[0].length - 1;
    const params = sliceBalanced(source, openIndex);
    const userParams = splitTopLevel(params)
      .map((param) => param.trim())
      .filter((param) => param !== "")
      .map((param) => {
        const colon = param.indexOf(":");
        if (colon < 0) return null;
        return { name: param.slice(0, colon).trim(), type: param.slice(colon + 1).trim() };
      })
      .filter(Boolean)
      .filter((param) => !/AppHandle|State<|Window|WebviewWindow/.test(param.type));
    if (userParams.length !== 1) continue;
    if (!/^(?:serde_json::)?Value$/.test(userParams[0].type)) continue;
    // 取该命令的 `#[tauri::command(...)]` 属性，判断是否需要驼峰化参数名。
    const attribute = source.slice(Math.max(0, m.index - 400), m.index);
    const renameAll = attribute.match(/rename_all\s*=\s*"([^"]+)"\s*\)/);
    const camel = renameAll && renameAll[1] === "camelCase";
    map.set(name, camel ? toCamelCase(userParams[0].name) : userParams[0].name);
  }
  return map;
}

/**
 * 提取 api.ts 中每个 `call("<cmd>", { … })` 的顶层键名。
 *
 * 只登记**对象字面量**调用；实参是变量（`call("x", args)`）时记 `null`，
 * 由调用方决定是报错还是豁免——静默跳过会让护栏悄悄失效。
 */
function extractCallArgKeys(apiTs) {
  const source = stripComments(apiTs);
  const map = new Map();
  const re = new RegExp(`${CALL_PREFIX}"([^"]+)"\\s*(,)?`, "g");
  let m;
  while ((m = re.exec(source)) !== null) {
    const cmd = m[1];
    const afterComma = m.index + m[0].length;
    if (!m[2]) {
      map.set(cmd, []);
      continue;
    }
    let i = afterComma;
    while (i < source.length && /\s/.test(source[i])) i += 1;
    if (source[i] !== "{") {
      map.set(cmd, null);
      continue;
    }
    map.set(cmd, topLevelKeys(sliceBalanced(source, i)));
  }
  return map;
}

function main() {
  const apiTs = read(API_TS);
  const screenshotTs = read(SCREENSHOT_DEMO_TS);
  const serverRs = read(SERVER_RS);
  const libRs = read(TAURI_LIB_RS);
  const commandsRs = read(TAURI_COMMANDS_RS);

  const routes = extractRoutes(apiTs);
  const callCmds = extractCallCommands(apiTs);
  const callArgKeys = extractCallArgKeys(apiTs);
  const demoReadCmds = extractDemoReadCommands(apiTs);
  const demoCaseCmds = extractDemoCaseCommands(screenshotTs);
  const serverRoutes = extractServerRoutes(serverRs);
  const invokeCmds = extractInvokeCommands(libRs);
  const singleValueCommands = extractSingleValueParamCommands(commandsRs);

  const routeCmds = new Set(routes.map((r) => r.cmd));
  const errors = [];
  const hints = [];

  // 1) 每个 call("<cmd>") 必须已在桌面端 invoke_handler 登记。
  for (const cmd of [...callCmds].sort()) {
    if (!invokeCmds.has(cmd)) {
      errors.push(
        `[desktop] call("${cmd}") 未在 src-tauri/src/lib.rs 的 invoke_handler 登记 → 桌面端会 command not found`,
      );
    }
  }

  // 2) 每个 call("<cmd>")（桌面专属豁免除外）必须有 ROUTES 条目。
  for (const cmd of [...callCmds].sort()) {
    if (!routeCmds.has(cmd) && !ROUTE_EXEMPT_COMMANDS.has(cmd)) {
      errors.push(
        `[webui] call("${cmd}") 缺少 ROUTES 路由条目 → webui 会抛「暂不支持该操作」`,
      );
    }
  }

  // 3) ROUTES 不得有死路由（没有任何 call("<cmd>") 使用它）。
  for (const cmd of [...routeCmds].sort()) {
    if (!callCmds.has(cmd)) {
      errors.push(`[webui] ROUTES["${cmd}"] 是死路由：没有任何 call("${cmd}") 使用它`);
    }
  }

  // 4) 每个 ROUTES 的 cmd 必须已在桌面端 invoke_handler 登记。
  for (const cmd of [...routeCmds].sort()) {
    if (!invokeCmds.has(cmd)) {
      errors.push(
        `[desktop] ROUTES["${cmd}"] 未在 src-tauri/src/lib.rs 的 invoke_handler 登记 → 桌面端会 command not found`,
      );
    }
  }

  // 5) 每个 ROUTES 的 path + method 必须与服务端路由表一致。
  for (const { cmd, method, path: routePath } of routes) {
    const methods = serverRoutes.get(routePath);
    if (!methods) {
      errors.push(
        `[server] ROUTES["${cmd}"] 的 path "${routePath}" 在 crates/buddy-switch-server/src/api.rs 中不存在`,
      );
      continue;
    }
    if (!methods.has(method)) {
      errors.push(
        `[server] ROUTES["${cmd}"] 以 ${method} 访问 "${routePath}"，但服务端该路由仅支持 [${[...methods]
          .sort()
          .join(", ")}]`,
      );
    }
  }

  // 6) 每个演示只读命令都必须有 screenshotDemoResponse 的 case 分支（漏 case 只在运行时才炸）。
  for (const cmd of [...demoReadCmds].sort()) {
    if (!demoCaseCmds.has(cmd)) {
      errors.push(
        `[demo] DEMO_READ_COMMANDS 含 "${cmd}"，但 src/lib/screenshot-demo.ts 的 screenshotDemoResponse 没有对应 case → 演示模式运行时会抛「演示模式缺少只读数据」`,
      );
    }
  }
  // 反向：case 分支不在 DEMO_READ_COMMANDS 中（可能由 call() 之外的路径演示化）—— 提示级，不失败。
  for (const cmd of [...demoCaseCmds].sort()) {
    if (!demoReadCmds.has(cmd)) {
      hints.push(
        `（提示）screenshot-demo 的 case "${cmd}" 不在 DEMO_READ_COMMANDS 中（可能由 call() 之外的路径演示化）`,
      );
    }
  }

  // 7) 「单 Value 参数」命令：前端必须把该参数名作为**唯一**的顶层键传过去。
  //
  // 这类命令的载荷形状在两侧源码里都是单一、机器可读的，所以能硬校验；
  // 校验失败的典型症状是「webui 正常、桌面端报 missing required key」。
  if (singleValueCommands.size === 0) {
    throw new Error(
      "未能从 src-tauri/src/commands.rs 解析出任何「单 Value 参数」命令 —— 第 7 条校验会静默失效",
    );
  }
  for (const [cmd, expectedKey] of [...singleValueCommands].sort()) {
    // 前端没调用的命令不涉及载荷形状（可能是内部 helper，或由其他通道调用）。
    if (!callCmds.has(cmd) || SINGLE_VALUE_SHAPE_EXEMPT.has(cmd)) continue;
    const keys = callArgKeys.get(cmd);
    if (keys === undefined || keys === null) {
      errors.push(
        `[desktop] call("${cmd}") 的实参不是可解析的对象字面量，无法校验参数名` +
          `（命令签名只接受一个 \`${expectedKey}: Value\`）→ 请改用对象字面量 \`{ ${expectedKey}: … }\`，` +
          `或说明理由后加入 SINGLE_VALUE_SHAPE_EXEMPT`,
      );
      continue;
    }
    if (keys.length !== 1 || keys[0] !== expectedKey) {
      errors.push(
        `[desktop] call("${cmd}") 的顶层键为 [${keys.join(", ") || "（无）"}]，` +
          `但命令签名只接受一个 \`${expectedKey}: Value\` 参数 → 桌面端会报 ` +
          `\`missing required key ${expectedKey}\`（webui 通道因后端兼容平铺，可能看不出问题）`,
      );
    }
  }

  if (hints.length > 0) {
    for (const hint of hints) process.stdout.write(`${hint}\n`);
  }

  if (errors.length > 0) {
    process.stderr.write(`API 契约校验失败（共 ${errors.length} 项）：\n`);
    for (const error of errors) process.stderr.write(`  - ${error}\n`);
    process.exit(1);
  }

  process.stdout.write(
    `API 契约校验通过：routes=${routes.length}，call=${callCmds.size}，` +
      `server=${serverRoutes.size}，invoke=${invokeCmds.size}，` +
      `demoRead=${demoReadCmds.size}，demoCase=${demoCaseCmds.size}，` +
      `单Value参数=${singleValueCommands.size}，` +
      `桌面专属豁免=${ROUTE_EXEMPT_COMMANDS.size}\n`,
  );
}

try {
  main();
} catch (error) {
  process.stderr.write(`API 契约校验无法执行：${error && error.message ? error.message : error}\n`);
  process.exit(1);
}
