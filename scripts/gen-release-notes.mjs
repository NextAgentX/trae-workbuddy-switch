#!/usr/bin/env node
/**
 * 生成 GitHub Release 的「版本变化内容」正文。
 *
 * 为什么自己生成、而**不用** `softprops/action-gh-release` 的
 * `generate_release_notes: true`：GitHub 那套自动生成只从 **Pull Request** 里取
 * 变更（标题、标签、贡献者）。本仓的提交是**直接推到 main**（不用 PR），实测那种
 * 情况下自动生成的结果只有一行 `**Full Changelog**: ...compare/...` —— 也就是
 * 「没有变化内容」。而本仓的提交**恰好**严格遵循 Conventional Commits（见
 * `AGENTS.md` → Git Commit Language），所以直接按前缀分组就是一份可用的中文变更说明。
 *
 * 输入：两个 ref 之间的提交（默认「上一个 tag」→「本次 tag」）。
 * 输出：Markdown 正文，写到 `--out`（默认 `release-notes.md`）。
 *
 * usage:
 *   node scripts/gen-release-notes.mjs --tag v2026.9.221126
 *   node scripts/gen-release-notes.mjs --tag v2026.9.221126 --from v2026.9.211636
 *   node scripts/gen-release-notes.mjs --tag v2026.9.221126 --repo NextAgentX/trae-workbuddy-switch
 *   node scripts/gen-release-notes.mjs --help
 *
 * ## 几处刻意选择（改动前先读）
 *
 * 1. **只取提交首行，不取正文。** 本仓的提交正文动辄几十行、且含大量内部实现细节与
 *    ⚠️ 警戒（那是写给维护者的）。Release 正文面向安装用户，一行一项即可。
 * 2. **不排除 merge 提交之外的东西，但排除 merge 本身**（`--no-merges`）：
 *    本仓是线性 history + squash 式提交；merge 提交的标题（`Merge pull request #N`）
 *    对用户没有信息量。
 * 3. **生成失败不阻断发布。** 变更说明是**附加信息**：缺失是「降级」，而发布失败是
 *    「事故」。因此本脚本任何异常都会退化为一段兜底说明 + `::warning::` 注解，
 *    退出码仍为 0。**不要**改成 `exit 1` —— 那会把「说明没生成出来」放大成
 *    「二进制发不出去」。
 * 4. **人工润色优先。** 若存在 `docs/releases/<tag>.md`（或去掉 `v` 前缀的同名文件），
 *    则**整篇采用**该文件、不做任何自动拼装 —— 需要人工措辞时用它，不要让脚本去猜。
 */

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const CUSTOM_NOTES_DIR = resolve(REPO_ROOT, "docs/releases");

/** 分组顺序即正文顺序：用户最关心的在前，维护性的在后。 */
const GROUPS = [
  { title: "🚀 新功能", types: ["feat"] },
  { title: "🐛 问题修复", types: ["fix"] },
  { title: "⚡ 性能优化", types: ["perf"] },
  { title: "♻️ 重构与内部调整", types: ["refactor"] },
  { title: "📝 文档", types: ["docs"] },
  { title: "🔧 构建与 CI", types: ["build", "ci"] },
  { title: "🧹 其他维护", types: ["chore", "style", "test", "revert"] },
];
const BREAKING_TITLE = "⚠️ 破坏性变更";
const FALLBACK_TITLE = "其他变更";

/** `type(scope)!: 描述`（容忍中文全角冒号，本仓曾混用过）。 */
const CONVENTIONAL = /^([A-Za-z]+)(?:\(([^)]*)\))?(!)?\s*[:：]\s*(.*)$/;

function git(args) {
  try {
    return execFileSync("git", args, {
      cwd: REPO_ROOT,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      maxBuffer: 64 * 1024 * 1024,
    }).trim();
  } catch (error) {
    // 只保留 stderr 的第一行：`execFileSync` 默认把**整条命令**（含 `%x1f` 这类
    // 控制字符）拼进 message，而下面要把这个原因写进 Release 正文与 GitHub 注解 ——
    // 注解是单行语义，多行内容会被截断或糊成一行。
    const stderr = error && error.stderr ? String(error.stderr).trim() : "";
    const first = stderr.split("\n").map((line) => line.trim()).filter(Boolean)[0];
    const detail =
      first || (error instanceof Error ? error.message.split("\n")[0] : String(error));
    throw new Error(`git ${args[0]} 失败：${detail}`);
  }
}

function gitOrEmpty(args) {
  try {
    return git(args);
  } catch {
    return "";
  }
}

function parseArgs(argv) {
  const opts = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--help" || arg === "-h") {
      opts.help = true;
      continue;
    }
    if (!arg.startsWith("--")) throw new Error(`无法识别的参数：${arg}`);
    const [flag, inline] = arg.slice(2).split("=");
    const key = flag.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
    const value = inline !== undefined ? inline : argv[i + 1];
    if (value === undefined || value.startsWith("--")) {
      throw new Error(`参数 --${flag} 缺少取值`);
    }
    if (inline === undefined) i += 1;
    opts[key] = value;
  }
  return opts;
}

/** `owner/repo`：CI 里有 GITHUB_REPOSITORY，本机则回落到 origin 远端。 */
function resolveRepo(opts) {
  if (opts.repo) return opts.repo.replace(/^https?:\/\/github\.com\//, "").replace(/\.git$/, "");
  if (process.env.GITHUB_REPOSITORY) return process.env.GITHUB_REPOSITORY;
  const url = gitOrEmpty(["remote", "get-url", "origin"]);
  const match = url.match(/github\.com[:/]([^/\s]+)\/([^/\s]+?)(?:\.git)?$/);
  return match ? `${match[1]}/${match[2]}` : "";
}

/**
 * 上一个 tag。优先 `git describe`（选**距离最近**的可达 tag）。
 * 它的失败是正常分支而不是异常：当前 tag 之前的提交若从未打过 tag
 * （本仓目前就只有 1 个 tag），`describe` 会 `fatal: No tags can describe`，
 * 此时回落到「所有可达 tag 里日期最新的那个」，仍无则视为首次发布。
 */
function previousTag(tag) {
  if (!tag) return "";
  const attempts = [
    ["describe", "--tags", "--abbrev=0", "--match", "v*", `${tag}^`],
    ["tag", "--list", "v*", "--merged", `${tag}^`, "--sort=-creatordate"],
  ];
  for (const args of attempts) {
    const first = gitOrEmpty(args).split("\n").map((line) => line.trim()).filter(Boolean)[0];
    if (first) return first;
  }
  return "";
}

function readCommits(range) {
  // ⚠️ 这里**故意用会抛错的 `git()`**，而不是 `gitOrEmpty`：range 里的 ref 写错
  // （tag 名打错、`fetch-depth: 1` 导致历史不完整）时，`git log` 会 exit 128，
  // 而吞掉错误就只剩「0 条提交」—— 正文会安静地变成「本次发布不含用户可见的
  // 功能变更」。那是**看着正常、实际错误**的结果。宁可走 main 的兜底分支，
  // 让 `::warning::` 把真实错因带出来。
  //
  // 合法的空区间（两个 ref 之间确实没有提交）`git log` 会 exit 0、输出为空 ——
  // 不需要额外区分，0 条提交本身就是那个分支。
  //
  // `%x1f` 分隔字段、`%x1e` 分隔记录：提交正文含换行与任意标点，
  // 只有非打印控制字符能安全做分隔（用 `|` / 空行分隔都会被正文内容撞上）。
  const raw = git(["log", "--no-merges", "--pretty=format:%H%x1f%s%x1f%b%x1e", range]);
  return raw
    .split("\x1e")
    .map((record) => record.replace(/^[\r\n]+/, ""))
    .filter(Boolean)
    .map((record) => {
      const [hash, subject = "", body = ""] = record.split("\x1f");
      return { hash: hash.trim(), subject: subject.trim(), body };
    });
}

function classify(commit) {
  const match = commit.subject.match(CONVENTIONAL);
  const breaking = /(^|\n)BREAKING[ -]CHANGE\s*:/.test(commit.body);
  if (!match) {
    return { hash: commit.hash, group: FALLBACK_TITLE, scope: "", text: commit.subject, breaking };
  }
  const [, type, scope = "", bang = "", text = ""] = match;
  const lowerType = type.toLowerCase();
  const group = GROUPS.find((candidate) => candidate.types.includes(lowerType));
  return {
    hash: commit.hash,
    group: group ? group.title : FALLBACK_TITLE,
    scope: scope.trim(),
    text: text.trim() || commit.subject,
    // 两种破坏性写法都认：type!: 与正文里的 BREAKING CHANGE:
    breaking: Boolean(bang) || breaking,
  };
}

function formatEntry(entry, repo) {
  const scope = entry.scope ? `**${entry.scope}**：` : "";
  const link = repo
    ? `（[${entry.hash.slice(0, 7)}](https://github.com/${repo}/commit/${entry.hash})）`
    : `（${entry.hash.slice(0, 7)}）`;
  return `- ${scope}${entry.text}${link}`;
}

function customNotes(tag) {
  if (!tag) return null;
  for (const name of [tag, tag.replace(/^v/, "")]) {
    const path = resolve(CUSTOM_NOTES_DIR, `${name}.md`);
    if (existsSync(path)) {
      const body = readFileSync(path, "utf8").trim();
      if (body) return { path, body };
    }
  }
  return null;
}

function render({ tag, prev, commits, repo }) {
  const groups = new Map();
  for (const commit of commits) {
    const entry = classify(commit);
    const key = entry.breaking ? BREAKING_TITLE : entry.group;
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(entry);
  }

  const orderedTitles = [
    BREAKING_TITLE,
    ...GROUPS.map((group) => group.title),
    FALLBACK_TITLE,
  ].filter((title) => groups.has(title));

  const scopeLabel = prev ? `\`${prev}\` → \`${tag}\`` : `首个带 tag 的版本 \`${tag}\``;
  const header = [
    `<!-- 本正文由 scripts/gen-release-notes.mjs 自动生成（分组依据：提交的 Conventional Commits 前缀）。`,
    `     需要人工措辞时，新建 docs/releases/${tag || "vX.Y.Z"}.md 即可整篇覆盖，无需改脚本。 -->`,
    "",
    `> 对比范围：${scopeLabel} · 共 ${commits.length} 项变更`,
    "",
  ];

  const body = orderedTitles.length
    ? orderedTitles.flatMap((title) => [
        `### ${title}`,
        "",
        ...groups.get(title).map((entry) => formatEntry(entry, repo)),
        "",
      ])
    : ["本次发布不含用户可见的功能变更（仅版本号与构建产物更新）。", ""];

  const footer = [];
  // 对比链接用 GitHub 的**三点**形式 `A...B`（其文档形式，必然可用）；本仓历史是
  // 线性的，它与上面 `git log A..B`（两点）取到的提交集合**完全一致**。
  // 若将来改成带合并的分支流（历史分叉），两者会差出「只在 A 上、不在 B 上」的
  // 提交 —— 届时这里要重新验证，别默认仍然等价。
  if (repo && prev) {
    footer.push(
      "",
      "---",
      "",
      `**完整变更对比**：https://github.com/${repo}/compare/${prev}...${tag}`,
    );
  } else if (repo) {
    footer.push("", "---", "", `**提交历史**：https://github.com/${repo}/commits/${tag}`);
  }
  footer.push(
    "**安装包**：Windows（NSIS）· macOS（DMG，Apple Silicon / Intel）· Linux（deb / AppImage）；桌面端也可在应用内直接更新。",
    "**许可**：PolyForm Noncommercial 1.0.0 —— 源码公开（source-available）· 非商业使用。",
    "",
  );

  return [...header, ...body, ...footer].join("\n");
}

function fallbackBody({ tag, prev, repo, reason }) {
  const lines = [
    "<!-- 由 scripts/gen-release-notes.mjs 兜底生成：变更说明本次未能自动生成。 -->",
    "",
    "本次发布未附带自动生成的变更清单。",
    "",
    reason ? `> 生成失败原因：${reason}` : "",
  ].filter((line) => line !== "");
  if (repo) {
    const link = prev
      ? `https://github.com/${repo}/compare/${prev}...${tag}`
      : `https://github.com/${repo}/commits/${tag}`;
    lines.push("", "---", "", `**完整变更**：${link}`);
  }
  return `${lines.join("\n")}\n`;
}

function usage() {
  return [
    "usage: node scripts/gen-release-notes.mjs [--tag <tag>] [--from <ref>] [--out <file>] [--repo <owner/repo>]",
    "",
    "  --tag   本次发布的 tag（默认 GITHUB_REF_NAME）",
    "  --from  对比起点的 ref（默认自动找上一个 tag；找不到即为首次发布）",
    "  --out   输出文件（默认 release-notes.md，相对仓库根目录）",
    "  --repo  GitHub 仓库坐标（默认 GITHUB_REPOSITORY，再回落 origin 远端）",
  ].join("\n");
}

function main(argv) {
  const opts = parseArgs(argv);
  if (opts.help) {
    console.log(usage());
    return 0;
  }

  const tag = opts.tag || process.env.GITHUB_REF_NAME || "";
  const outPath = resolve(REPO_ROOT, opts.out || "release-notes.md");
  const repo = resolveRepo(opts);

  const write = (body) => {
    writeFileSync(outPath, body.endsWith("\n") ? body : `${body}\n`, "utf8");
    console.log(`gen-release-notes: 已写入 ${outPath}`);
  };

  // 提到 try 外面：兜底分支也要用它拼 compare 链接，否则「自动找上一个 tag」时
  // 兜底正文会丢掉对比链接（`opts.from` 为空）。
  let prev = opts.from !== undefined ? opts.from : "";

  try {
    const custom = customNotes(tag);
    if (custom) {
      write(custom.body);
      console.log(`gen-release-notes: 使用人工撰写的 ${custom.path}（未自动拼装）`);
      return 0;
    }

    prev = opts.from !== undefined ? opts.from : previousTag(tag);
    const range = prev ? `${prev}..${tag || "HEAD"}` : tag || "HEAD";
    const commits = readCommits(range);
    console.log(
      `gen-release-notes: 范围 ${range}，解析出 ${commits.length} 条提交` +
        (prev ? "" : "（无上一个 tag，按首次发布处理）"),
    );
    write(render({ tag, prev, commits, repo }));
    return 0;
  } catch (error) {
    // 见头部注释第 3 条：降级，不阻断发布。
    const reason = error instanceof Error ? error.message : String(error);
    // ⚠️ 兜底写入**必须**自己再包一层。失败点就在 `write` 里时（输出路径不可写 /
    // 磁盘满 / 路径不存在），在 catch 里直接调 `write` 等于**原地再抛一次**，而这次
    // 没有任何人接 ⇒ 变成未捕获异常，`process.exit(main(...))` 根本不会执行，
    // node 以 **1** 退出并打出一整段栈 —— 与头部注释第 3 条「退出码仍为 0」**正好相反**。
    // 更坑的是 `::warning::` 已经先打出去了，CI 日志看起来像「降级成功」，实际那一步是红的。
    try {
      write(fallbackBody({ tag, prev, repo, reason }));
    } catch (fallbackError) {
      // 走到这里说明 `--out` 一定没落地。CI 里 `body_path` 指向不存在的文件，
      // 发布步骤**无论如何**都会失败 ⇒ 此时「响亮失败 + 说清原因」远好过一段栈。
      // 注意：这**不是**第 3 条要避免的那种「把降级放大成事故」—— 降级已经不可能了。
      const detail =
        fallbackError instanceof Error ? fallbackError.message : String(fallbackError);
      console.log(
        `::error title=版本变更说明无法写入::${reason}；兜底写入同样失败：${detail}`,
      );
      return 1;
    }
    console.log(`::warning title=版本变更说明生成失败::${reason}`);
    return 0;
  }
}

process.exit(main(process.argv.slice(2)));
