//! 长期记忆（Memory）跨账号合并去重。
//!
//! 对照参考实现 `workbuddy-account-migrate/scripts/migrate.py::migrate_memory`，
//! 但**修掉了它的主要缺陷**：参考版按「整行字符串完全相等」去重，缩进、行尾空白、
//! 列表符号（`-` / `*`）、标题层级（`#` 数量）任一不同就会被判为新内容，
//! 反复迁移会把同一段落累积成多份。
//!
//! 本模块的判定单位是**规范化后的行**：
//!   1. 去掉行首行尾空白；
//!   2. 折叠行内连续空白为单个空格；
//!   3. 剥掉行首的列表符号（`-` `*` `+`）与有序列表序号（`1.` `2)`）；
//!   4. 剥掉标题的 `#` 前缀——`## 偏好` 与 `### 偏好` 视为同一段内容；
//!   5. 空行与纯分隔线（`---` / `***` / `___`）不参与去重判定。
//!
//! 纯函数（[`normalize_memory_line`] / [`merge_memory_text`]）不依赖文件系统，
//! 便于无 UI 环境单测；[`merge_memory_files`] 负责读写 `{user_id}_memory.md`。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::modules::region::Region;

/// 记忆文件相对于 session 数据目录的子目录名。
const MEMORY_DIR: &str = "memory";

/// 单次合并追加的段落上限（防止一次性导入超大文件把目标文件撑爆）。
pub const MEMORY_MERGE_MAX_APPENDED: usize = 2000;

/// Memory 合并结果计数。
///
/// 非 `Copy`：`backup` 持有路径。若调用方只需计数，用字段读取即可。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryMergeResult {
    /// 目标的原始行数（不含空行）。
    pub target_lines: usize,
    /// 源的原始行数（不含空行）。
    pub source_lines: usize,
    /// 最终追加的**新**行数。
    pub appended: usize,
    /// 因已存在于目标而跳过的行数。
    pub skipped_duplicate: usize,
    /// 实际改写时，目标文件改前原文的备份路径；未改写或无目标文件时为 `None`。
    pub backup: Option<PathBuf>,
}

impl MemoryMergeResult {
    /// 是否有实际写入（无新增时可跳过落盘）。
    pub fn changed(&self) -> bool {
        self.appended > 0
    }
}

/// `{user_id}_memory.md` 所在目录（按 region 隔离）。
pub fn memory_dir_for(region: Region) -> PathBuf {
    crate::modules::session::session_data_dir(region).join(MEMORY_DIR)
}

/// `{user_id}_memory.md` 路径。
pub fn memory_file_for(region: Region, uid: &str) -> PathBuf {
    memory_dir_for(region).join(format!("{uid}_memory.md"))
}

/// 规范化一行记忆文本，作为去重指纹。返回 `None` 表示该行不参与去重判定
/// （空行、纯分隔线）。
///
/// 规范化是**幂等**的：`normalize(normalize(x)) == normalize(x)`。
pub fn normalize_memory_line(line: &str) -> Option<String> {
    // 1) 先把「纯分隔线」摘出去。分隔线在剥列表符号之后会退化成单个 `-`，
    //    因此必须在剥符号**之前**判定，否则 `- - -` 会被当成正文 `-`。
    let trimmed_raw = line.trim();
    if trimmed_raw.is_empty() || is_separator_line(trimmed_raw) {
        return None;
    }

    // 2) 折叠行内空白（含制表符 / 全角空格）并 trim。
    let text = collapse_whitespace(trimmed_raw);
    if text.is_empty() {
        return None;
    }
    let mut text = text;

    // 3) 先剥标题前缀（`#` 必须后跟空白或行尾，避免误伤 `#tag`）。
    while let Some(rest) = strip_leading_hashes(&text) {
        text = collapse_whitespace(rest);
        if text.is_empty() || is_separator_line(&text) {
            return None;
        }
    }

    // 4) 再剥无序 / 有序列表符号。标题剥完后的 `- item` 也要剥。
    while let Some(rest) = strip_leading_bullet(&text) {
        text = collapse_whitespace(rest);
        if text.is_empty() || is_separator_line(&text) {
            return None;
        }
    }

    (!text.is_empty() && !is_separator_line(&text)).then_some(text)
}

/// 把行内所有空白字符折叠为单个空格并 trim 两端。
fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            in_space = true;
        } else {
            if in_space && !out.is_empty() {
                out.push(' ');
            }
            in_space = false;
            out.push(ch);
        }
    }
    out
}

/// 剥掉一个标题前缀：`# Heading` → `Heading`。`#tag`（无空白）不视为标题。
fn strip_leading_hashes(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('#')?;
    // 允许连续多个 `#`（`##` / `###`）。
    let rest = rest.trim_start_matches('#');
    if rest.is_empty() {
        return Some("");
    }
    if rest.starts_with(' ') {
        Some(rest)
    } else {
        None
    }
}

/// 剥掉一个列表符号：`- item` / `* item` / `+ item` / `1. item` / `2) item`。
fn strip_leading_bullet(text: &str) -> Option<&str> {
    let first = text.chars().next()?;
    if matches!(first, '-' | '*' | '+') {
        let rest = &text[first.len_utf8()..];
        // 单独一个 `-` 或 `---`（分隔线）由 is_separator_line 处理，这里要求后跟空白。
        return rest.starts_with(' ').then_some(rest);
    }
    // 有序列表：连续数字 + `.` 或 `)` + 空白。
    let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let rest = &text[digits.len()..];
    let rest = rest.strip_prefix('.').or_else(|| rest.strip_prefix(')'))?;
    rest.starts_with(' ').then_some(rest)
}

/// 是否为 Markdown 分隔线（`---` / `***` / `___`，允许空格间隔）。
fn is_separator_line(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let mut chars = compact.chars();
    let Some(marker) = chars.next() else {
        return false;
    };
    if !matches!(marker, '-' | '*' | '_') {
        return false;
    }
    let rest: String = chars.collect();
    rest.len() >= 2 && rest.chars().all(|c| c == marker)
}

/// 纯函数：把源记忆文本合并进目标记忆文本，返回合并后的文本与计数。
///
/// 语义：
/// - 目标为空（无有效行）→ 直接返回源内容（原样保留缩进与格式）；
/// - 目标非空 → 逐行比对规范化指纹，只追加目标中不存在的新行；
/// - 追加块带 `## 迁移自 {来源}` 分隔标题，便于用户回溯；
/// - 追加行数上限为 [`MEMORY_MERGE_MAX_APPENDED`]。
pub fn merge_memory_text(
    target: &str,
    source: &str,
    source_label: &str,
) -> (String, MemoryMergeResult) {
    let target_lines: Vec<&str> = target
        .lines()
        .filter(|l| normalize_memory_line(l).is_some())
        .collect();
    let source_lines: Vec<&str> = source
        .lines()
        .filter(|l| normalize_memory_line(l).is_some())
        .collect();

    let mut result = MemoryMergeResult {
        target_lines: target_lines.len(),
        source_lines: source_lines.len(),
        appended: 0,
        skipped_duplicate: 0,
        backup: None,
    };

    // 目标为空：直接采用源内容（不做逐行改写，保留原始缩进）。
    if target_lines.is_empty() {
        let trimmed = source.trim();
        if trimmed.is_empty() {
            return (target.to_string(), result);
        }
        result.appended = source_lines.len().min(MEMORY_MERGE_MAX_APPENDED);
        return (trimmed.to_string(), result);
    }

    // 已存在的规范化指纹集合（目标 + 本次已接受的行，避免源内部重复）。
    let mut seen: HashSet<String> = target_lines
        .iter()
        .filter_map(|l| normalize_memory_line(l))
        .collect();

    let mut accepted: Vec<String> = Vec::new();
    for raw in source.lines() {
        let Some(fingerprint) = normalize_memory_line(raw) else {
            continue;
        };
        if !seen.insert(fingerprint) {
            result.skipped_duplicate += 1;
            continue;
        }
        if accepted.len() >= MEMORY_MERGE_MAX_APPENDED {
            result.skipped_duplicate += 1;
            continue;
        }
        // 保留源行的原始缩进（去掉行尾空白），只把去重交给指纹。
        accepted.push(raw.trim_end().to_string());
    }

    if accepted.is_empty() {
        return (target.to_string(), result);
    }

    result.appended = accepted.len();
    let label = source_label.trim();
    let mut out = target.trim_end().to_string();
    out.push_str("\n\n---\n");
    if !label.is_empty() {
        out.push_str(&format!("## 迁移自 {label}\n\n"));
    }
    out.push_str(&accepted.join("\n"));
    out.push('\n');
    (out, result)
}

/// 合并两个账号的 Memory 文件（同一版本内）：读源 → 合并到目标 → 原子写回。
///
/// 源文件不存在 / 为空 → 不写入，返回 `changed() == false`。
pub fn merge_memory_files(
    region: Region,
    source_uid: &str,
    target_uid: &str,
) -> Result<MemoryMergeResult, String> {
    merge_memory_files_cross(region, source_uid, region, target_uid)
}

/// 跨版本合并：把 `source_region` 下源账号的记忆合并进 `target_region` 下目标账号。
///
/// 源与目标**版本不同**时不因 uid 字面相同而拒绝 —— 两版 uid 不同源，同文只是巧合，
/// 两侧也是两个不同文件，不存在自我覆盖；**同版本且 uid 相同**才是真正的自我覆盖。
pub fn merge_memory_files_cross(
    source_region: Region,
    source_uid: &str,
    target_region: Region,
    target_uid: &str,
) -> Result<MemoryMergeResult, String> {
    if source_region == target_region && source_uid.trim() == target_uid.trim() {
        return Err("源账号与目标账号相同".to_string());
    }
    let source_path = memory_file_for(source_region, source_uid);
    if !source_path.is_file() {
        return Ok(MemoryMergeResult::default());
    }
    let source = std::fs::read_to_string(&source_path)
        .map_err(|e| format!("读取源记忆失败（{}）：{e}", source_path.display()))?;
    if source.trim().is_empty() {
        return Ok(MemoryMergeResult::default());
    }

    let target_path = memory_file_for(target_region, target_uid);
    let target = if target_path.is_file() {
        std::fs::read_to_string(&target_path)
            .map_err(|e| format!("读取目标记忆失败（{}）：{e}", target_path.display()))?
    } else {
        String::new()
    };

    let label = shorten_uid(source_uid);
    let (merged, mut result) = merge_memory_text(&target, &source, &label);
    if result.changed() {
        // 采纳参考实现的「必须先备份」安全规则：改写目标前先留原文。
        // 只在**目标原本存在且实际改写**时备份 —— 目标不存在则无原文可留，
        // 幂等重跑也不会产生空备份。
        if target_path.is_file() {
            result.backup = backup_target_memory(&target_path, &target);
        }
        write_memory_atomically(&target_path, &merged)?;
    }
    Ok(result)
}

/// 备份目标记忆文件改前原文到 `{store}/backups/memory/{时间戳}/`。
///
/// 备份失败**不阻断**合并（返回 `None`）：迁移的价值高于备份留存，
/// 但失败会在报告里体现为 `backup: null`，便于用户判断是否需要自行留档。
fn backup_target_memory(target_path: &Path, original: &str) -> Option<PathBuf> {
    let file_name = target_path.file_name()?.to_string_lossy().to_string();
    let dir = crate::modules::config::backup_dir()
        .join("memory")
        .join(crate::modules::config::utc_iso());
    std::fs::create_dir_all(&dir).ok()?;
    let dest = dir.join(file_name);
    crate::modules::config::atomic_write(&dest, original).ok()?;
    Some(dest)
}

/// 写入记忆文件：与 `config::atomic_write` 同源，避免半截文件。
fn write_memory_atomically(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建记忆目录失败（{}）：{e}", parent.display()))?;
    }
    crate::modules::config::atomic_write(path, content)
        .map_err(|e| format!("写入记忆失败（{}）：{e}", path.display()))
}

/// 迁移标注里的 uid 缩写：前 12 字符 + 省略号（对照参考实现）。
fn shorten_uid(uid: &str) -> String {
    let trimmed = uid.trim();
    if trimmed.chars().count() <= 12 {
        return trimmed.to_string();
    }
    let head: String = trimmed.chars().take(12).collect();
    format!("{head}...")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_collapses_whitespace_and_strips_bullets_and_headings() {
        // 缩进 / 行尾空白 / 行内多空格 —— 同一内容。
        assert_eq!(
            normalize_memory_line("   - 用户偏好表格展示  "),
            normalize_memory_line("- 用户偏好表格展示")
        );
        assert_eq!(
            normalize_memory_line("用户偏好   表格展示"),
            Some("用户偏好 表格展示".to_string())
        );
        // 列表符号不影响指纹。
        assert_eq!(
            normalize_memory_line("- 偏好 A"),
            normalize_memory_line("* 偏好 A")
        );
        assert_eq!(
            normalize_memory_line("1. 偏好 A"),
            normalize_memory_line("2) 偏好 A")
        );
        // 标题层级不影响指纹，且标题与正文同内容也算同一段。
        assert_eq!(
            normalize_memory_line("## 偏好"),
            normalize_memory_line("### 偏好")
        );
        assert_eq!(
            normalize_memory_line("# 偏好"),
            normalize_memory_line("偏好")
        );
    }

    #[test]
    fn normalize_rejects_blank_and_separator_lines() {
        for raw in ["", "   ", "\t", "---", "***", "___", "- - -", "  ---  "] {
            assert_eq!(normalize_memory_line(raw), None, "应被忽略：{raw:?}");
        }
    }

    #[test]
    fn normalize_does_not_mangle_hashtags_or_negative_numbers() {
        // `#tag` 没有空白，不视为标题。
        assert_eq!(normalize_memory_line("#tag"), Some("#tag".to_string()));
        // `-5` 没有空白，不视为列表项。
        assert_eq!(normalize_memory_line("-5 度"), Some("-5 度".to_string()));
    }

    #[test]
    fn normalize_is_idempotent() {
        for raw in [
            "  ##  - 用户偏好  ",
            "- 1. 嵌套项",
            "普通文本",
            "### 标题",
        ] {
            let once = normalize_memory_line(raw);
            if let Some(once) = once {
                assert_eq!(
                    normalize_memory_line(&once),
                    Some(once.clone()),
                    "规范化必须幂等：{raw:?}"
                );
            }
        }
    }

    #[test]
    fn merge_into_empty_target_copies_source_verbatim() {
        let source = "# 记忆\n\n- 偏好表格\n  - 缩进保留\n";
        let (merged, result) = merge_memory_text("", source, "uid-old");
        // 有效行：# 记忆 / - 偏好表格 / - 缩进保留
        assert_eq!(result.appended, 3);
        assert_eq!(result.skipped_duplicate, 0);
        assert!(merged.contains("  - 缩进保留"), "缩进应原样保留：{merged}");
        assert!(!merged.contains("迁移自"), "目标为空时不应加迁移标题");
        assert!(!merged.contains("---"), "目标为空时不应插入分隔线");
    }

    #[test]
    fn merge_skips_lines_differing_only_in_formatting() {
        // 目标用 `## 偏好` + `- 表格`；源用 `### 偏好` + `* 表格` + 缩进。
        // 参考实现的逐行精确匹配会把这 2 行都当成新内容，本实现必须全部跳过。
        let target = "## 偏好\n- 表格展示\n";
        let source = "   ### 偏好\n  * 表格展示\n";
        let (merged, result) = merge_memory_text(target, source, "uid-old");
        assert_eq!(result.appended, 0, "仅格式不同不得判为新内容");
        assert_eq!(result.skipped_duplicate, 2);
        assert_eq!(merged, target, "无新增时不得改动目标文本");
    }

    #[test]
    fn merge_appends_only_genuinely_new_lines() {
        let target = "## 偏好\n- 表格展示\n";
        let source = "## 偏好\n- 表格展示\n- 直接执行指令\n";
        let (merged, result) = merge_memory_text(target, source, "uid-old");
        // 源有效行 3 条：`偏好` / `表格展示` / `直接执行指令`；前两条命中目标。
        assert_eq!(result.appended, 1);
        assert_eq!(result.skipped_duplicate, 2);
        assert!(merged.contains("迁移自 uid-old"));
        assert!(merged.contains("- 直接执行指令"));
        // 既有内容不得被重复写入。
        assert_eq!(merged.matches("表格展示").count(), 1);
    }

    #[test]
    fn merge_deduplicates_within_source_itself() {
        let target = "已有内容\n";
        let source = "- 新增项\n* 新增项\n  *  新增项  \n";
        let (_, result) = merge_memory_text(target, source, "uid-old");
        assert_eq!(result.appended, 1, "源内部重复只应追加一次");
        assert_eq!(result.skipped_duplicate, 2);
    }

    #[test]
    fn merge_is_idempotent_across_repeated_runs() {
        let target = "## 偏好\n- 表格\n";
        let source = "## 偏好\n- 表格\n- 结构化表达\n";
        let (once, first) = merge_memory_text(target, source, "uid-old");
        assert_eq!(first.appended, 1);
        // 再合并一次：源的有效行都已存在，不得再追加。
        let (twice, second) = merge_memory_text(&once, source, "uid-old");
        assert_eq!(second.appended, 0, "二次合并必须完全幂等");
        assert_eq!(second.skipped_duplicate, 3, "源 3 条有效行全部命中");
        assert_eq!(twice, once);
    }

    #[test]
    fn merge_result_counts_reflect_non_blank_lines() {
        let target = "# 标题\n\n\n---\n内容\n";
        let source = "\n# 标题\n\n新内容\n";
        let (_, result) = merge_memory_text(target, source, "u");
        assert_eq!(result.target_lines, 2, "空行与分隔线不计入");
        assert_eq!(result.source_lines, 2);
        assert_eq!(result.appended, 1);
    }

    #[test]
    fn merge_into_empty_source_is_noop() {
        let target = "已有内容\n";
        for source in ["", "   \n\n", "---\n***\n"] {
            let (merged, result) = merge_memory_text(target, source, "u");
            assert_eq!(result.appended, 0, "源无有效行时不应追加：{source:?}");
            assert_eq!(merged, target);
        }
    }

    #[test]
    fn merge_caps_appended_lines() {
        let target = "种子\n";
        let source: String = (0..MEMORY_MERGE_MAX_APPENDED + 50)
            .map(|i| format!("- 条目 {i}\n"))
            .collect();
        let (_, result) = merge_memory_text(target, &source, "u");
        assert_eq!(result.appended, MEMORY_MERGE_MAX_APPENDED);
        assert_eq!(result.skipped_duplicate, 50);
    }

    /// 记忆文件按 region 隔离、按 uid 命名。
    ///
    /// ## 为什么必须持 `env_lock()`
    ///
    /// 断言的两侧（`memory_file_for` 与 `memory_dir_for`）都是**无入参**的路径函数，
    /// 每次调用都重读进程级 `BUDDY_SWITCH_HOME`。lib 单测在同一进程里并行跑，
    /// 只要有别的用例（`HomeOverrideGuard` 系列）在这两次读取之间换掉该变量，
    /// 就会出现「左侧真实 home、右侧临时 home」的**假失败**
    /// （2026-09-24 实测：新增一条网络端到端用例改变调度时序后本用例开始红，
    /// 报 `left: …\Temp\buddy-switch-current-nickname-…\.workbuddy\memory`）。
    /// 修法是让本用例与所有改 home 的用例互斥，而不是给断言加容错。
    #[test]
    fn memory_paths_are_region_scoped_and_named_by_uid() {
        let _lock = crate::modules::config::env_lock();
        let cn = memory_file_for(Region::Cn, "uid-a");
        let global = memory_file_for(Region::Global, "uid-a");
        assert_ne!(cn, global, "CN / Global 记忆文件必须隔离");
        assert!(cn.ends_with(std::path::Path::new("memory").join("uid-a_memory.md")));
        assert!(global.ends_with(std::path::Path::new("memory").join("uid-a_memory.md")));
        assert_eq!(cn.parent(), Some(memory_dir_for(Region::Cn).as_path()));
    }

    #[test]
    fn merge_same_uid_is_rejected() {
        assert!(merge_memory_files(Region::Cn, "uid-a", "uid-a").is_err());
        // 首尾空白差异也应视为同一账号。
        assert!(merge_memory_files(Region::Cn, " uid-a ", "uid-a").is_err());
    }

    #[test]
    fn shorten_uid_truncates_long_ids() {
        assert_eq!(shorten_uid("short"), "short");
        assert_eq!(shorten_uid("0123456789ab"), "0123456789ab");
        assert_eq!(shorten_uid("0123456789abcdef"), "0123456789ab...");
    }
}
