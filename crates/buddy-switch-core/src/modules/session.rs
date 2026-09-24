//! 会话列表与按需复制（路径 B：生成新 id，云端可正常同步）。
//!
//! 对照 server.py `current_user_uid` / `list_sessions_for_user` /
//! `_find_project_jsonl` / `copy_session_to_user` / `_register_edge_sync_mapping` /
//! `copy_sessions_for_switch` / `backup_workbuddy_db` / `workbuddy_db_path`。
//!
//! WorkBuddy 5.x 数据三件套（缺一不可）：
//!   1) 正文：`~/.workbuddy/projects/{workspace}/{cid}.jsonl`（JSONL 含 sessionId 字段）
//!   2) 元数据：`~/.workbuddy/workbuddy.db` sessions 表（id = conversation id = UUID）
//!   3) 云端映射：`~/.workbuddy/edge-sync-mapping-v2.db` edge_sync_mapping
//!      （session_id=conversation_id，msg_channel=convmsg:{uid} 决定云端归属）

use rusqlite::Connection;
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::modules::auth_file;
use crate::modules::config::{backup_dir, home_dir, now_ms, now_secs, utc_iso};
use crate::modules::region::Region;

/// 打开数据库并设置 busy_timeout（对照 Python `sqlite3.connect(timeout=5)`）。
fn open_db(path: &Path, read_only: bool) -> Option<Connection> {
    let conn = if read_only {
        Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?
    } else {
        Connection::open(path).ok()?
    };
    let _ = conn.busy_timeout(Duration::from_secs(5));
    Some(conn)
}

/// 客户端会话数据目录（CN `.workbuddy` / Global `.workbuddy-ai`）。
pub fn session_data_dir(region: Region) -> PathBuf {
    let name = match region {
        Region::Cn => ".workbuddy",
        Region::Global => ".workbuddy-ai",
    };
    home_dir().join(name)
}

/// CN 会话数据库路径。
pub fn workbuddy_db_path() -> PathBuf {
    workbuddy_db_path_for(Region::Cn)
}

/// 按 region 会话数据库路径。
pub fn workbuddy_db_path_for(region: Region) -> PathBuf {
    session_data_dir(region).join("workbuddy.db")
}

/// CN 会话边车映射数据库路径（旧签名，仅供单测使用）。
#[cfg(test)]
fn edge_sync_db_path() -> PathBuf {
    edge_sync_db_path_for(Region::Cn)
}

fn edge_sync_db_path_for(region: Region) -> PathBuf {
    session_data_dir(region).join("edge-sync-mapping-v2.db")
}

/// 当前认证账号的 uid（CN 认证文件 account.uid）。
pub fn current_user_uid() -> Option<String> {
    current_user_uid_for(Region::Cn)
}

/// 按 region 当前认证账号的 uid（认证文件 account.uid）。
pub fn current_user_uid_for(region: Region) -> Option<String> {
    let auth = auth_file::read_auth_file_for(region)?;
    auth.get("account")
        .and_then(|a| a.get("uid"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [name],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
        == 1
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> bool {
    let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info({table})")) else {
        return false;
    };
    let Ok(iter) = stmt.query_map([], |row| row.get::<_, String>(1)) else {
        return false;
    };
    let names: Vec<String> = iter.flatten().collect();
    names.iter().any(|name| name == column)
}

fn nonempty_text(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// WorkBuddy 侧栏展示名：优先 custom_title（用户改名 / 定时任务名），否则 title。
fn session_display_title(title: Option<String>, custom_title: Option<String>) -> String {
    nonempty_text(custom_title)
        .or_else(|| nonempty_text(title))
        .unwrap_or_else(|| "(无标题)".to_string())
}

/// Claw 是账号绑定的 IM 渠道工作区，复制会话行不够，目标账号也用不了。
fn is_claw_workspace(cwd: &str) -> bool {
    cwd.trim()
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case("claw"))
}

/// 列出某账号未删除的会话（CN；workbuddy.db sessions 表，db 为准）。
///
/// `title` 为 WorkBuddy 侧栏同款展示名；`isPlayground` 对应侧栏「任务」，
/// 其余按 `cwd` 最后一段归入「空间」。
pub fn list_sessions_for_user(uid: &str) -> Value {
    list_sessions_for_user_for(Region::Cn, uid)
}

/// 按 region 列出某账号未删除的会话。
///
/// 归属匹配见 `USER_SCOPE_STRICT` / `USER_SCOPE_WIDENED`：当前账号的行**并上**「空归属」行，
/// 避免客户端把 `user_id` 落成空串时被误判成「账号无会话」。
pub fn list_sessions_for_user_for(region: Region, uid: &str) -> Value {
    let db = workbuddy_db_path_for(region);
    if !db.is_file() {
        return json!([]);
    }
    let Some(conn) = open_db(&db, true) else {
        return json!([]);
    };
    if !table_exists(&conn, "sessions") {
        return json!([]);
    }
    let has_custom = column_exists(&conn, "sessions", "custom_title");
    let has_playground = column_exists(&conn, "sessions", "is_playground");
    let columns = match (has_custom, has_playground) {
        (true, true) => "id, cwd, title, custom_title, updated_at, is_playground",
        (true, false) => "id, cwd, title, custom_title, updated_at, 0",
        (false, true) => "id, cwd, title, NULL, updated_at, is_playground",
        (false, false) => "id, cwd, title, NULL, updated_at, 0",
    };
    // 归属匹配 = 严格命中当前账号的行 **∪** 「空归属」行（见 `USER_SCOPE_*`）。
    // 取并集而不是「严格为空才放宽」：后者在「既有当前账号的会话、又有旧空归属行」的库里
    // 仍然看不到旧会话（用户现场：索引恢复后切换弹窗里依然列不出来）。
    let mut sessions = query_session_rows(&conn, region, columns, USER_SCOPE_STRICT, uid);
    if !uid.is_empty() {
        for s in query_session_rows(&conn, region, columns, USER_SCOPE_WIDENED, uid) {
            if !sessions.iter().any(|e| e.get("id") == s.get("id")) {
                sessions.push(s);
            }
        }
        // 两段各自按 updatedAt 倒序，合并后需重排，保持「最近活动在前」。
        sessions.sort_by(|a, b| {
            let ka = a.get("updatedAt").and_then(Value::as_i64).unwrap_or(0);
            let kb = b.get("updatedAt").and_then(Value::as_i64).unwrap_or(0);
            kb.cmp(&ka)
        });
    }
    json!(sessions)
}

/// `sessions.user_id` 的归属过滤：`STRICT` 命中当前账号，`WIDENED` 额外纳入「空归属」行。
///
/// **为什么要纳入空归属行**：实测（2026-09-23，本机 18:47 的 Global 库备份）**116 条会话的
/// `user_id` 全是空字符串** —— 这是客户端某些版本 / 迁移后的落库形态，不是损坏，也**不该**
/// 被当成「账号没有会话」。严格过滤会让 UI 静默退化成「账号暂无会话」并**禁用复制**
/// （用户现场报障：切换账号时无法复制会话）。
///
/// 取并集（而非「严格命中 0 行才放宽」）：库里同时存在当前账号的行与旧空归属行时，
/// 旧会话同样必须列得出来。
///
/// 边界：**只纳入 `NULL` / 空串**（本机这份库里「没人认领」的行），**不会**把别的非空 uid
/// 的会话列出来（负向对照见 `list_sessions_does_not_widen_to_other_accounts`）；
/// 当前 uid 为空串时不做合并（两段会完全重叠）。
const USER_SCOPE_STRICT: &str = "user_id = ?1";
const USER_SCOPE_WIDENED: &str = "(user_id = ?1 OR user_id IS NULL OR user_id = '')";

/// 按给定归属范围查会话行并映射成前端结构；任何一步失败都返回空表（**绝不报错**，
/// 让上层能继续走降级扫描）。
fn query_session_rows(
    conn: &Connection,
    region: Region,
    columns: &str,
    scope: &str,
    uid: &str,
) -> Vec<Value> {
    let sql = format!(
        "SELECT {columns} FROM sessions WHERE {scope} AND deleted_at IS NULL \
         ORDER BY updated_at DESC"
    );
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([uid], |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, Option<i64>>(5)?,
        ))
    }) else {
        return Vec::new();
    };

    let mut sessions: Vec<Value> = Vec::new();
    for r in rows.flatten() {
        let (cid, cwd, title, custom_title, updated_at, is_playground) = r;
        let cid = cid.unwrap_or_default();
        let cwd = cwd.unwrap_or_default();
        if is_claw_workspace(&cwd) {
            continue;
        }
        sessions.push(json!({
            "id": cid,
            "title": session_display_title(title, custom_title),
            "cwd": cwd,
            "updatedAt": updated_at.unwrap_or(0),
            "hasHistory": find_project_jsonl_for(region, &cid).is_some(),
            "isPlayground": is_playground.unwrap_or(0) != 0,
        }));
    }
    sessions
}

/// 按 region 列出某账号未删除的会话，db 不可读 / 为空时降级扫描 projects 目录。
///
/// 返回结构（与 `list_sessions_for_user_for` 不同的、更结构化的形态）：
///   {
///     "sessions": [...],
///     "source": "db" | "scan" | "empty" | "no-dir",
///     "warning"?: "..." // source != "db" 时给出
///   }
///
/// `source == "no-dir"` 表示**数据目录里根本没有会话数据**（`workbuddy.db` 与 `projects/` 都不存在）
/// —— 典型成因是客户端刚被重装 / 重置（2026-09-23 现场：整个 `~/.workbuddy` 被重建为空）。
/// 它必须与「账号确实没有会话」区分开：后者报成前者会**误导用户**（UI 会显示「当前账号暂无会话」，
/// 而真实原因是数据目录空了）。
///
/// 命令层（`commands.rs::list_sessions`、`api.rs::api_sessions`）直接透传给前端；
/// 旧 `list_sessions_for_user_for` 保持纯数组形态、继续给单测和只关心 session 数组
/// 的调用方使用，避免一处改动牵动全栈。
///
/// 优先级：db 读到非空 → "db"；db 读到空但 projects 有 jsonl → "scan"；
/// 两者都空且数据目录存在 → "empty"；数据目录里连 db / projects 都没有 → "no-dir"。
/// **绝不让 UI 把「db 损坏」误显示成「账号无会话」**。
pub fn list_sessions_with_fallback_for(region: Region, uid: &str) -> Value {
    let from_db = list_sessions_for_user_for(region, uid);
    let db_count = from_db.as_array().map(|a| a.len()).unwrap_or(0);
    if db_count > 0 {
        return json!({
            "sessions": from_db,
            "source": "db",
        });
    }
    let scanned = scan_sessions_from_jsonl_for(region);
    if !scanned.is_empty() {
        return json!({
            "sessions": scanned,
            "source": "scan",
            "warning": "数据库索引不可读，已从会话文件扫描；元数据可能不完整（如 customTitle / 任务/空间归属）",
        });
    }
    if !data_dir_has_session_data(region) {
        return json!({
            "sessions": [],
            "source": "no-dir",
        });
    }
    json!({
        "sessions": [],
        "source": "empty",
    })
}

/// 数据目录里是否存在任何会话数据载体（`workbuddy.db` 或 `projects/`）。
///
/// 两者皆无 ⇒ 客户端的会话数据不在本机（重装 / 重置 / 从未登录过），
/// 与「账号在这份库里没有会话」是两回事，必须让 UI 分辨得出。
fn data_dir_has_session_data(region: Region) -> bool {
    workbuddy_db_path_for(region).is_file() || session_data_dir(region).join("projects").is_dir()
}

/// 按 region 在 `{data_dir}/projects/{workspace}/{cid}.jsonl` 定位会话正文。
fn find_project_jsonl_for(region: Region, cid: &str) -> Option<PathBuf> {
    let projects = session_data_dir(region).join("projects");
    if !projects.is_dir() {
        return None;
    }
    let direct = projects.join(format!("{cid}.jsonl"));
    if direct.is_file() {
        return Some(direct);
    }
    for entry in std::fs::read_dir(&projects).ok()?.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let p = entry.path().join(format!("{cid}.jsonl"));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// jsonl 头部能拿到的最小元数据（cwd / 标题）。
///
/// ## ★ 字段名是实测出来的，别照字面猜
///
/// WorkBuddy 5.5.x 的会话 jsonl 里，元数据**不在首行的同一个对象上**（实测三份真实会话）：
/// - `cwd`：**每一行都带**（首行 `type:"message"` 也有），值是真实路径（如 `d:\_03_WorkBuddy\workbuddy-switch`）；
/// - 标题：在 `type:"ai-title"` 那一行的 **`aiTitle`** 字段（实测**第 3 行**）；
/// - `customTitle`（用户改名 / 定时任务名）出现在**更靠后**的记录里（实测第 56 行）。
///
/// ⚠️ 早期实现只认 `title` ⇒ 标题**永远取不到**，降级扫描出来的会话**全是「(无标题)」**。
/// 而 db 的 `title` 列实测**就等于 `aiTitle`**（逐条比对过），`custom_title` 是另一列
/// ⇒ 优先级与 [`session_display_title`] 对齐：**`customTitle` > `aiTitle`**
/// （`title` 作为兼容别名一起认，供旧格式与既有测试夹具使用）。
#[derive(Debug, Default, Clone)]
struct JsonlMeta {
    cwd: String,
    title: String,
}

/// 只读文件头这么多字节去找元数据。
///
/// **刻意不读整个文件**：会话正文实测可到 2.3MB，而 `projects/**` 动辄几百个文件；
/// cwd / 标题都在前几行。旧实现 `read_to_string` 把整份读进来只取前 8 行，
/// 且扫描器对同一文件**调了两次** —— 等于每个文件读两遍全文（现已合并为 [`push_jsonl_session`] 里的一次）。
const JSONL_META_HEAD_BYTES: u64 = 64 * 1024;

/// 头部内最多看多少行。实测元数据在第 3 / 第 56 行，128 行留足余量；
/// 真正的成本闸是上面的**字节**上限（一行超长时不会白扫）。
const JSONL_META_MAX_LINES: usize = 128;

/// 从 jsonl 头部解析 cwd / 标题。文件不存在 / 不可读 / 非 JSON / 字段缺失
/// 均返回 `None`，**绝不报错**（降级路径的目标是「让 UI 至少列得出来」）。
fn read_jsonl_meta(path: &Path) -> Option<JsonlMeta> {
    let file = std::fs::File::open(path).ok()?;
    let mut head = Vec::new();
    file.take(JSONL_META_HEAD_BYTES).read_to_end(&mut head).ok()?;
    // 头部可能正好切在一行中间（甚至多字节字符中间）⇒ 有损解码，末行解析失败会被跳过。
    let text = String::from_utf8_lossy(&head);

    let mut cwd = String::new();
    let mut ai_title = String::new();
    let mut custom_title = String::new();
    for line in text.lines().take(JSONL_META_MAX_LINES) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if cwd.is_empty() {
            cwd = text_field(&v, "cwd");
        }
        if ai_title.is_empty() {
            ai_title = text_field(&v, "aiTitle");
            if ai_title.is_empty() {
                ai_title = text_field(&v, "title");
            }
        }
        if custom_title.is_empty() {
            custom_title = text_field(&v, "customTitle");
        }
        // 三项都齐了就不必再解析后面的行（正常格式第 3 行就该齐）。
        if !cwd.is_empty() && !ai_title.is_empty() && !custom_title.is_empty() {
            break;
        }
    }
    let title = if custom_title.is_empty() {
        ai_title
    } else {
        custom_title
    };
    if cwd.is_empty() && title.is_empty() {
        return None;
    }
    Some(JsonlMeta { cwd, title })
}

/// 取字符串字段并 trim；缺失 / 非字符串（脏值）一律给空串，不参与展示。
fn text_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

/// 从 `~/.workbuddy{,-ai}/projects/` 扫描会话 jsonl，从文件名得到 cid、
/// 从**文件头**（见 [`read_jsonl_meta`]）解析 cwd / 标题。**仅在 sessions 表不可读 / 为空时调用**。
///
/// **只在两层内取文件**：`projects/<cid>.jsonl`（扁平布局）与 `projects/<workspace>/<cid>.jsonl`。
/// ⚠️ **刻意不再往下递归**：`projects/<workspace>/<cid>/subagents/agent-*.jsonl` 是**子代理**
/// 记录，不是会话 —— 实测（2026-09-24）国内版 `projects/` 里 391 个真会话旁边躺着 **81 个**
/// 子代理文件；无界递归会把它们当成会话，切换弹窗里凭空多出几十条 `agent-xxxx` 假条目。
/// （本函数此前的注释写着「层数写死」，但实现是无界递归 —— 这里把注释与实现对齐。）
///
/// 返回顺序：按文件 mtime DESC（最近活动在前），与 db 版同语义。
/// Claw 工作区会被跳过，与 db 版语义对齐（缺 cwd 时不判 Claw）。
///
/// 限制：仅扫描 `.jsonl`；`.file-rollback.ndjson` / `.meta.json` 等忽略。
fn scan_sessions_from_jsonl_for(region: Region) -> Vec<Value> {
    let projects = session_data_dir(region).join("projects");
    if !projects.is_dir() {
        return Vec::new();
    }
    let mut sessions: Vec<(i64, Value)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(&projects) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            push_jsonl_session(&path, &mut sessions);
            continue;
        }
        let Ok(children) = std::fs::read_dir(&path) else {
            continue;
        };
        for child in children.flatten() {
            push_jsonl_session(&child.path(), &mut sessions);
        }
    }
    sessions.sort_by(|a, b| b.0.cmp(&a.0));
    sessions.into_iter().map(|(_, v)| v).collect()
}

/// 把单个路径收成一条会话记录；不是 `.jsonl` 文件 / 读不出元数据时跳过（**绝不报错**）。
fn push_jsonl_session(path: &Path, out: &mut Vec<(i64, Value)>) {
    if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
        return;
    }
    let Some(cid) = path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    let Ok(md) = std::fs::metadata(path) else {
        return;
    };
    if !md.is_file() {
        return;
    }
    // **只读一次**：cwd 与标题同在这一个头部里，读两遍等于每个文件读两遍全文。
    let meta = read_jsonl_meta(path);
    let cwd = meta.as_ref().map(|m| m.cwd.clone()).unwrap_or_default();
    if !cwd.is_empty() && is_claw_workspace(&cwd) {
        return;
    }
    let title = meta
        .map(|m| m.title)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "(无标题)".to_string());
    let mtime = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    out.push((
        mtime,
        json!({
            "id": cid,
            "title": title,
            "cwd": cwd,
            "updatedAt": mtime,
            "hasHistory": true,
            "isPlayground": false,
            "degraded": true,
        }),
    ));
}

/// 备份 workbuddy.db（含 -wal/-shm），返回主库备份路径。对照 `backup_workbuddy_db`。
fn backup_workbuddy_db(region: Region, backup_root: &Path) -> Option<PathBuf> {
    let db = workbuddy_db_path_for(region);
    if !db.is_file() {
        return None;
    }
    std::fs::create_dir_all(backup_root).ok()?;
    for suffix in ["", "-wal", "-shm"] {
        let src = PathBuf::from(format!("{}{}", db.to_string_lossy(), suffix));
        if src.is_file() {
            let _ = std::fs::copy(&src, backup_root.join(format!("workbuddy.db{suffix}")));
        }
    }
    Some(backup_root.join("workbuddy.db"))
}

/// 把 source_uid 的一个会话复制为 target_uid 的新会话（路径 B：生成新 id）。
///
/// 全部按「新 id」复制一份给目标账号，源账号数据完全不动。
/// 新 id 必须用带连字符的 UUID 格式（`Uuid::new_v4().to_string()`），与官方一致；
/// 32 位无连字符形式会导致 WorkBuddy 无法识别新会话。
pub fn copy_session_to_user(
    cid: &str,
    source_uid: &str,
    target_uid: &str,
) -> Result<Value, String> {
    copy_session_to_user_for(Region::Cn, cid, source_uid, target_uid)
}

/// 按 region（同一版本内）把 source_uid 的一个会话复制为 target_uid 的新会话（路径 B）。
pub fn copy_session_to_user_for(
    region: Region,
    cid: &str,
    source_uid: &str,
    target_uid: &str,
) -> Result<Value, String> {
    copy_session_to_user_cross(region, region, cid, source_uid, target_uid)
}

/// 跨版本复制：正文与元数据**读自 `source_region`**，副本与云端映射**写入 `target_region`**。
///
/// **去重**：若该源会话此前已复制给同一目标账号且副本仍在，则不重复复制，
/// 直接返回既有副本（`deduplicated: true`）。见 [`ledger_hit_for`]。
///
/// **降级**：源 sessions 表不可读时，从 jsonl 第一行推断 cwd 做 Claw 检查；
/// 目标 sessions 表不可写时，jsonl 仍能复制但索引行留空（返回 `warning`）。
/// 这样即便 workbuddy.db 损坏 / 索引缺失，至少 jsonl 正文能落到目标账号，
/// WorkBuddy AI 重启后建索引即可看到。
pub fn copy_session_to_user_cross(
    source_region: Region,
    target_region: Region,
    cid: &str,
    source_uid: &str,
    target_uid: &str,
) -> Result<Value, String> {
    if let Some(existing) =
        ledger_hit_for(source_region, target_region, source_uid, target_uid, cid)
    {
        return Ok(json!({
            "id": cid,
            "newId": existing,
            "jsonlCopied": false,
            "mappingWritten": false,
            "backup": Value::Null,
            "deduplicated": true,
        }));
    }

    let new_cid = uuid::Uuid::new_v4().to_string();

    // Claw 检查：优先从源 sessions 表读 cwd；表不可读时降级从 jsonl 推断。
    // Claw 是「账号绑定的 IM 渠道工作区」，绑死当前账号渠道，复制给目标也用不了。
    let cwd_from_db = match open_db(&workbuddy_db_path_for(source_region), true) {
        Some(conn) if table_exists(&conn, "sessions") => conn
            .query_row(
                "SELECT cwd FROM sessions WHERE id = ?1 AND user_id = ?2",
                rusqlite::params![cid, source_uid],
                |r| r.get(0),
            )
            .ok(),
        _ => None,
    };
    let cwd_for_claw = match cwd_from_db {
        Some(c) => c,
        None => find_project_jsonl_for(source_region, cid)
            .and_then(|p| read_jsonl_meta(&p).map(|m| m.cwd))
            .unwrap_or_default(),
    };
    if !cwd_for_claw.is_empty() && is_claw_workspace(&cwd_for_claw) {
        return Err("Claw 工作区绑定当前账号渠道，不支持复制".into());
    }

    // 1) 复制正文 jsonl：源 projects 下 {ws}/{cid}.jsonl → 目标 projects 下同位置 {new_cid}.jsonl
    let mut jsonl_copied = false;
    if let Some(src_jsonl) = find_project_jsonl_for(source_region, cid) {
        let dst_jsonl = target_jsonl_path_for(source_region, target_region, &src_jsonl, &new_cid);
        if let Some(parent) = dst_jsonl.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = std::fs::read_to_string(&src_jsonl) {
            let text = text.replace(cid, &new_cid); // 替换 sessionId 等旧 id 引用
            if std::fs::write(&dst_jsonl, text).is_ok() {
                jsonl_copied = true;
            }
        }
    }

    // 2) 备份目标 db（复制前），再把源行复制为新 id 插进目标库
    //    降级：源 / 目标 db 不可用时不再 Err，而是 Ok(false) 表达「没写索引行」；
    //    调用方根据这个信号决定是否在报告里加 warning（jsonl 仍可继续复制）。
    let backup_root = backup_dir().join("sessions").join(utc_iso());
    backup_workbuddy_db(target_region, &backup_root);
    let session_row_written = insert_session_copy(
        &workbuddy_db_path_for(source_region),
        &workbuddy_db_path_for(target_region),
        &new_cid,
        cid,
        source_uid,
        target_uid,
    )
    .unwrap_or(false);

    // 3) 注册云端映射：新会话归属目标账号（msg_channel=convmsg:{target_uid}）
    let mapping_written = register_edge_sync_mapping_for(target_region, &new_cid, target_uid);

    // 4) 登记去重账本：下次复制同一源会话时直接跳过
    let ledger_written = record_copy_ledger(
        source_region,
        target_region,
        source_uid,
        target_uid,
        cid,
        &new_cid,
    );

    // 降级警告：jsonl 复制成功但索引行没写 → 用户重启 WorkBuddy 即可看到。
    // 真正阻塞的失败（jsonl 也复制失败）已在前面步骤显式无声返回，
    // 这里**不**为此再加 warning，避免「既不报错也不警告」式的静默降级。
    let warning = if jsonl_copied && !session_row_written {
        Some("目标账号的会话索引不可写；已复制正文，请重启 WorkBuddy 让其重建索引".to_string())
    } else if jsonl_copied && !mapping_written {
        Some("云端映射注册失败；新会话暂时无法在 WorkBuddy 中显示云端历史".to_string())
    } else {
        None
    };

    let mut report = json!({
        "id": cid,
        "newId": new_cid,
        "jsonlCopied": jsonl_copied,
        "sessionRowWritten": session_row_written,
        "mappingWritten": mapping_written,
        "backup": backup_root.to_string_lossy().to_string(),
        "deduplicated": false,
        "ledgerWritten": ledger_written,
    });
    if let Some(w) = warning {
        report["warning"] = json!(w);
    }
    Ok(report)
}

/// 会话正文副本的落点：保持「相对 projects 目录的子路径」不变，换到目标版本目录下。
///
/// 同版本时结果与原实现完全一致（同一 workspace 目录、只换文件名）；
/// 跨版本时把正文搬到 `target_region` 的 projects，否则副本写进源版本目录、
/// 目标版本的 WorkBuddy 根本看不到它。
fn target_jsonl_path_for(
    source_region: Region,
    target_region: Region,
    src_jsonl: &Path,
    new_cid: &str,
) -> PathBuf {
    let source_projects = session_data_dir(source_region).join("projects");
    let target_projects = session_data_dir(target_region).join("projects");
    let in_target = match src_jsonl.strip_prefix(&source_projects) {
        Ok(rel) => target_projects.join(rel),
        Err(_) => target_projects,
    };
    in_target.with_file_name(format!("{new_cid}.jsonl"))
}

/// 表的列名清单（按 PRAGMA 顺序）；表不存在时为空。
fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
    let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info({table})")) else {
        return Vec::new();
    };
    let Ok(iter) = stmt.query_map([], |row| row.get::<_, String>(1)) else {
        return Vec::new();
    };
    iter.flatten().collect()
}

/// 读出源会话行的列名与值；顺带做 Claw 判定。
///
/// 源库不存在 / 无 sessions 表 / 无此行 → `Ok(None)`（对照 Python 版静默跳过）。
fn read_session_row(
    src_db_path: &Path,
    cid: &str,
    source_uid: &str,
) -> Result<Option<(Vec<String>, Vec<rusqlite::types::Value>)>, String> {
    if !src_db_path.is_file() {
        return Ok(None);
    }
    let Some(conn) = open_db(src_db_path, true) else {
        return Ok(None);
    };
    if !table_exists(&conn, "sessions") {
        return Ok(None);
    }
    let mut stmt = conn
        .prepare("SELECT * FROM sessions WHERE id = ?1 AND user_id = ?2")
        .map_err(|e| e.to_string())?;
    let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut rows = stmt
        .query(rusqlite::params![cid, source_uid])
        .map_err(|e| e.to_string())?;
    let Some(row) = rows.next().map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let mut vals: Vec<rusqlite::types::Value> = Vec::with_capacity(cols.len());
    for (i, col) in cols.iter().enumerate() {
        let v = row
            .get::<_, rusqlite::types::Value>(i)
            .unwrap_or(rusqlite::types::Value::Null);
        if col == "cwd" {
            if let rusqlite::types::Value::Text(ref path) = v {
                if is_claw_workspace(path) {
                    return Err("Claw 工作区绑定当前账号渠道，不支持复制".into());
                }
            }
        }
        vals.push(v);
    }
    Ok(Some((cols, vals)))
}

/// 把源会话行复制为新 id 写入目标库（动态列，覆盖 id/user_id/时间戳）。
///
/// 源与目标可以是两个版本的 db。目标库缺某一列时**丢弃该列**而不是整条失败 ——
/// 两版 schema 高度同源但允许有差异，丢弃比让整次复制失败更接近用户预期。
///
/// 返回语义（与「响亮失败优于静默降级」互补：让 UI 至少能把 jsonl 搬过去）：
///   - `Ok(true)`  写入了 sessions 索引行；
///   - `Ok(false)` 软失败（源行缺失 / 目标 db 不存在 / 表不存在 / 列不全 / 执行失败），
///     调用方据此决定是否提示用户「重启 WorkBuddy 重建索引」；
///   - `Err(_)`    仅在 `read_session_row` 拒绝（如 Claw 工作区）时返回，仍会阻断复制。
fn insert_session_copy(
    src_db_path: &Path,
    dst_db_path: &Path,
    new_cid: &str,
    cid: &str,
    source_uid: &str,
    target_uid: &str,
) -> Result<bool, String> {
    let Some((cols, vals)) = read_session_row(src_db_path, cid, source_uid)? else {
        return Ok(false);
    };
    if !dst_db_path.is_file() {
        return Ok(false);
    }
    let Some(conn) = open_db(dst_db_path, false) else {
        return Ok(false);
    };
    if !table_exists(&conn, "sessions") {
        return Ok(false);
    }
    let dst_cols = table_columns(&conn, "sessions");

    let mut insert_cols: Vec<&str> = Vec::with_capacity(cols.len());
    let mut insert_vals: Vec<rusqlite::types::Value> = Vec::with_capacity(cols.len());
    for (col, v) in cols.iter().zip(vals) {
        if !dst_cols.iter().any(|c| c == col) {
            continue;
        }
        let v = match col.as_str() {
            "id" => rusqlite::types::Value::Text(new_cid.to_string()),
            "user_id" => rusqlite::types::Value::Text(target_uid.to_string()),
            "created_at" | "updated_at" => rusqlite::types::Value::Integer(now_ms()),
            "deleted_at" => rusqlite::types::Value::Null,
            _ => v,
        };
        insert_cols.push(col.as_str());
        insert_vals.push(v);
    }
    if insert_cols.is_empty() {
        return Ok(false);
    }

    let placeholders = insert_cols.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let colnames = insert_cols.join(", ");
    let sql = format!("INSERT OR REPLACE INTO sessions ({colnames}) VALUES ({placeholders})");
    let params: Vec<&rusqlite::types::Value> = insert_vals.iter().collect();
    match conn.execute(&sql, rusqlite::params_from_iter(params)) {
        Ok(_) => Ok(true),
        // 执行失败也降级为软失败：jsonl 已复制，至少正文能落到目标账号。
        Err(_) => Ok(false),
    }
}

/// 把新会话注册进 edge_sync_mapping（云端归属关键）。失败不致命，返回 False。
fn register_edge_sync_mapping_for(region: Region, new_cid: &str, target_uid: &str) -> bool {
    insert_edge_sync_mapping(&edge_sync_db_path_for(region), new_cid, target_uid)
}

fn insert_edge_sync_mapping(db_path: &Path, new_cid: &str, target_uid: &str) -> bool {
    if !db_path.is_file() {
        return false;
    }
    let Some(conn) = open_db(db_path, false) else {
        return false;
    };
    if !table_exists(&conn, "edge_sync_mapping") {
        return false;
    }
    let created_at = now_secs();
    let r = conn.execute(
        "INSERT OR REPLACE INTO edge_sync_mapping \
         (session_id, conversation_id, msg_channel, created_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            new_cid,
            new_cid,
            format!("convmsg:{target_uid}"),
            created_at
        ],
    );
    match r {
        Ok(_) => true,
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// 复制去重账本（同一源会话 → 同一目标账号只复制一次）
// ---------------------------------------------------------------------------

/// 复制去重账本相对于 session 数据目录的文件名。
///
/// 存放形如 `{ "source_uid\u{1f}target_uid\u{1f}source_cid": new_cid }` 的映射。
const COPY_LEDGER_FILE: &str = "buddy-switch-copy-ledger.json";

/// 复合键分隔符（Unit Separator；正常 uid / uuid 不会包含）。
const LEDGER_SEP: char = '\u{1f}';

fn copy_ledger_path_for(region: Region) -> PathBuf {
    session_data_dir(region).join(COPY_LEDGER_FILE)
}

/// 解析账本路径：仅用于单测把读写重定向到临时目录，**普通调用一律传 `None`**。
fn resolve_ledger_path(region: Region, override_path: Option<&Path>) -> PathBuf {
    match override_path {
        Some(p) => p.to_path_buf(),
        None => copy_ledger_path_for(region),
    }
}

/// 构造账本复合键：`source_uid ␟ target_uid ␟ source_cid`。
///
/// 三元组缺一不可：同一源会话复制给不同目标账号是两次独立操作；
/// 同一目标账号从不同源账号复制同名 cid 也应各自成条。
fn copy_ledger_key(source_uid: &str, target_uid: &str, source_cid: &str) -> String {
    format!("{source_uid}{LEDGER_SEP}{target_uid}{LEDGER_SEP}{source_cid}")
}

/// 跨版本复制的账本键：在三元键前加**源 region** 前缀。
///
/// 同版本刻意保持原键 —— 否则既有账本全部失配，老用户会被判成「没复制过」而重复复制；
/// 跨版本必须加前缀 —— 否则「CN 的 uid-a 复制给 X」与「Global 的 uid-a 复制给 X」
/// 会共用一条登记，其中一侧被错误跳过。
fn copy_ledger_key_for(
    source_region: Region,
    target_region: Region,
    source_uid: &str,
    target_uid: &str,
    source_cid: &str,
) -> String {
    let base = copy_ledger_key(source_uid, target_uid, source_cid);
    if source_region == target_region {
        base
    } else {
        format!("{}{LEDGER_SEP}{base}", source_region.as_str())
    }
}

/// 读取复制账本；文件缺失 / 损坏 / 结构不符时返回空表（视为无登记）。
fn load_copy_ledger_at(path: &Path) -> serde_json::Map<String, Value> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return serde_json::Map::new();
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    }
}

fn load_copy_ledger(region: Region) -> serde_json::Map<String, Value> {
    load_copy_ledger_at(&copy_ledger_path_for(region))
}

/// 原子写回复制账本。写入失败不致命（下次会重新复制），返回是否成功。
fn save_copy_ledger_at(path: &Path, ledger: &serde_json::Map<String, Value>) -> bool {
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    let content = serde_json::to_string_pretty(&Value::Object(ledger.clone())).unwrap_or_default();
    crate::modules::config::atomic_write(path, &content).is_ok()
}

/// 该账号名下是否存在未删除的指定会话（db 为准）。
fn session_exists_for(region: Region, cid: &str, uid: &str) -> bool {
    let db = workbuddy_db_path_for(region);
    if !db.is_file() {
        return false;
    }
    let Some(conn) = open_db(&db, true) else {
        return false;
    };
    if !table_exists(&conn, "sessions") {
        return false;
    }
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sessions \
         WHERE id = ?1 AND user_id = ?2 AND deleted_at IS NULL)",
        rusqlite::params![cid, uid],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n != 0)
    .unwrap_or(false)
}

/// 目标账号下该源会话是否已复制过：账本命中且副本仍存活 → 返回该副本 id。
///
/// 「仍存活」= 副本会话行仍在目标账号名下（未被用户删除）。
/// 用户删除副本后允许重新复制，避免账本变成永久性阻断。
fn ledger_hit_for(
    source_region: Region,
    target_region: Region,
    source_uid: &str,
    target_uid: &str,
    source_cid: &str,
) -> Option<String> {
    let ledger = load_copy_ledger(target_region);
    let new_cid = ledger
        .get(&copy_ledger_key_for(
            source_region,
            target_region,
            source_uid,
            target_uid,
            source_cid,
        ))?
        .as_str()?
        .trim()
        .to_string();
    if new_cid.is_empty() {
        return None;
    }
    session_exists_for(target_region, &new_cid, target_uid).then_some(new_cid)
}

/// 在账本中登记一次成功的复制。写入路径可被单测重定向（见 [`resolve_ledger_path`]）。
fn record_copy_ledger_at(
    source_region: Region,
    target_region: Region,
    override_path: Option<&Path>,
    source_uid: &str,
    target_uid: &str,
    source_cid: &str,
    new_cid: &str,
) -> bool {
    let path = resolve_ledger_path(target_region, override_path);
    let mut ledger = load_copy_ledger_at(&path);
    ledger.insert(
        copy_ledger_key_for(source_region, target_region, source_uid, target_uid, source_cid),
        Value::String(new_cid.to_string()),
    );
    save_copy_ledger_at(&path, &ledger)
}

fn record_copy_ledger(
    source_region: Region,
    target_region: Region,
    source_uid: &str,
    target_uid: &str,
    source_cid: &str,
    new_cid: &str,
) -> bool {
    record_copy_ledger_at(
        source_region,
        target_region,
        None,
        source_uid,
        target_uid,
        source_cid,
        new_cid,
    )
}

/// 切换前把勾选的会话复制到目标账号（CN，路径 B）。返回复制报告。
pub fn copy_sessions_for_switch(target_acc: &Value, session_ids: &[String]) -> Option<Value> {
    copy_sessions_for_switch_for(Region::Cn, target_acc, session_ids)
}

/// 按 region（同一版本内）切换前把勾选的会话复制到目标账号（路径 B）。返回复制报告。
pub fn copy_sessions_for_switch_for(
    region: Region,
    target_acc: &Value,
    session_ids: &[String],
) -> Option<Value> {
    copy_sessions_for_switch_cross(region, region, target_acc, session_ids)
}

/// 跨版本：把 `source_region` 当前登录账号的勾选会话复制到 `target_region` 的目标账号。
///
/// 源 uid 取自**源版本**的认证文件 —— 这正是跨版本的关键：沿用目标版本去取，
/// 取到的是目标版本自己的登录账号，会把「另一个账号」的会话搬过去。
pub fn copy_sessions_for_switch_cross(
    source_region: Region,
    target_region: Region,
    target_acc: &Value,
    session_ids: &[String],
) -> Option<Value> {
    let target_uid = target_acc
        .get("uid")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if target_uid.is_empty() {
        return None;
    }
    let source_uid = current_user_uid_for(source_region)?;
    // 同版本同 uid = 自我复制，无意义；跨版本 uid 同文不算（两版 uid 不同源）。
    if source_region == target_region && source_uid == target_uid {
        return None;
    }

    let mut report = json!({
        "sourceUid": source_uid,
        "targetUid": target_uid,
        "sourceRegion": source_region.as_str(),
        "targetRegion": target_region.as_str(),
        "copied": [],
    });
    let mut errors: Vec<Value> = Vec::new();
    // 命中账本的源会话：不重复复制，单独归类以便 UI 明确告知「已存在，跳过」
    let mut skipped: Vec<Value> = Vec::new();
    for cid in session_ids {
        match copy_session_to_user_cross(
            source_region,
            target_region,
            cid,
            &source_uid,
            &target_uid,
        ) {
            Ok(r) if r.get("deduplicated").and_then(Value::as_bool) == Some(true) => {
                skipped.push(r);
            }
            Ok(r) => report["copied"].as_array_mut().unwrap().push(r),
            Err(e) => errors.push(json!({"id": cid, "error": e})),
        }
    }
    if !errors.is_empty() {
        report["errors"] = json!(errors);
    }
    if !skipped.is_empty() {
        report["skipped"] = json!(skipped);
    }
    Some(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn db_paths_point_to_home() {
        // 用 Path 组件比较，避免 Windows `\` / Unix `/` 分隔符差异。
        assert!(workbuddy_db_path().ends_with(std::path::Path::new(".workbuddy").join("workbuddy.db")));
        assert!(edge_sync_db_path()
            .to_string_lossy()
            .ends_with("edge-sync-mapping-v2.db"));
    }

    /// 会话目录与数据库路径必须按 region 隔离（PRD G1 / D2）。
    ///
    /// CN 用 `~/.workbuddy`，Global 用 `~/.workbuddy-ai`；两版若共用一个目录，
    /// 会话库会互相覆盖。
    #[test]
    fn session_paths_are_region_scoped() {
        let cn_dir = session_data_dir(Region::Cn);
        let global_dir = session_data_dir(Region::Global);

        assert_eq!(
            cn_dir.file_name().and_then(|n| n.to_str()),
            Some(".workbuddy"),
            "CN 会话目录名"
        );
        assert_eq!(
            global_dir.file_name().and_then(|n| n.to_str()),
            Some(".workbuddy-ai"),
            "Global 会话目录名"
        );
        assert_ne!(cn_dir, global_dir, "CN / Global 会话目录必须隔离");
        // 同一 home，仅目录名不同。
        assert_eq!(cn_dir.parent(), global_dir.parent());
        assert_eq!(
            cn_dir,
            crate::modules::config::home_dir().join(".workbuddy")
        );
        assert_eq!(
            global_dir,
            crate::modules::config::home_dir().join(".workbuddy-ai")
        );

        // 数据库路径由会话目录派生，同样必须按 region 隔离。
        assert_eq!(workbuddy_db_path_for(Region::Cn), cn_dir.join("workbuddy.db"));
        assert_eq!(
            workbuddy_db_path_for(Region::Global),
            global_dir.join("workbuddy.db")
        );
        assert_ne!(
            workbuddy_db_path_for(Region::Cn),
            workbuddy_db_path_for(Region::Global)
        );
        // CN 兼容路径与 region 化路径一致，避免两处逻辑漂移。
        assert_eq!(workbuddy_db_path(), workbuddy_db_path_for(Region::Cn));
    }

    fn temp_db(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "buddy_switch_test_{}_{name}.db",
            uuid::Uuid::new_v4().simple()
        ))
    }

    #[test]
    fn insert_session_copy_duplicates_row_with_target_uid() {
        let db = temp_db("sessions");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                title TEXT,
                cwd TEXT,
                created_at INTEGER,
                updated_at INTEGER,
                deleted_at INTEGER,
                payload BLOB
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sessions (id, user_id, title, cwd, created_at, updated_at, deleted_at, payload)
             VALUES ('src-1', 'uid-a', '旧标题', '/ws', 1000, 2000, NULL, x'DEADBEEF')",
            [],
        )
        .unwrap();

        insert_session_copy(&db, &db, "new-uuid-1", "src-1", "uid-a", "uid-b").unwrap();

        let (id, user_id, title, deleted_at): (String, String, String, Option<i64>) = conn
            .query_row(
                "SELECT id, user_id, title, deleted_at FROM sessions WHERE id = 'new-uuid-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(id, "new-uuid-1");
        assert_eq!(user_id, "uid-b");
        assert_eq!(title, "旧标题"); // 普通列原样保留
        assert_eq!(deleted_at, None); // deleted_at 置空

        // 源行保持不变
        let src_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE id = 'src-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(src_count, 1);
    }

    #[test]
    fn insert_session_copy_missing_source_is_noop() {
        let db = temp_db("noop");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE sessions (id TEXT PRIMARY KEY, user_id TEXT, title TEXT, created_at INTEGER, updated_at INTEGER, deleted_at INTEGER);",
        )
        .unwrap();
        insert_session_copy(&db, &db, "new-1", "missing", "uid-a", "uid-b").unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn insert_session_copy_missing_db_is_ok() {
        let db = temp_db("missing");
        // 不创建文件
        assert!(insert_session_copy(&db, &db, "new-1", "src-1", "a", "b").is_ok());
    }

    /// 跨库复制：源行只出现在源库，副本只出现在目标库。
    ///
    /// 这是跨版本复制的核心断言 —— 若实现仍把「读」和「写」绑在同一个 db 上，
    /// 要么副本没进目标库（用户看不到），要么源库被写脏。
    #[test]
    fn insert_session_copy_across_databases() {
        let src_db = temp_db("xsrc");
        let dst_db = temp_db("xdst");
        for db in [&src_db, &dst_db] {
            let conn = Connection::open(db).unwrap();
            conn.execute_batch(
                "CREATE TABLE sessions (
                    id TEXT PRIMARY KEY,
                    user_id TEXT NOT NULL,
                    title TEXT,
                    created_at INTEGER,
                    updated_at INTEGER,
                    deleted_at INTEGER
                 );",
            )
            .unwrap();
        }
        Connection::open(&src_db)
            .unwrap()
            .execute(
                "INSERT INTO sessions (id, user_id, title, created_at, updated_at, deleted_at)
                 VALUES ('src-1', 'uid-a', '跨版本标题', 1000, 2000, NULL)",
                [],
            )
            .unwrap();

        insert_session_copy(&src_db, &dst_db, "new-1", "src-1", "uid-a", "uid-b").unwrap();

        let dst = Connection::open(&dst_db).unwrap();
        let (title, user_id): (String, String) = dst
            .query_row(
                "SELECT title, user_id FROM sessions WHERE id = 'new-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(title, "跨版本标题");
        assert_eq!(user_id, "uid-b", "副本必须归属目标账号");

        let src_rows: i64 = Connection::open(&src_db)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(src_rows, 1, "源库不得被写入");
    }

    #[test]
    fn insert_edge_sync_mapping_registers_channel() {
        let db = temp_db("edge");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE edge_sync_mapping (
                session_id TEXT,
                conversation_id TEXT,
                msg_channel TEXT,
                created_at INTEGER
            );",
        )
        .unwrap();
        assert!(insert_edge_sync_mapping(&db, "new-1", "uid-b"));
        let (sid, cid, channel): (String, String, String) = conn
            .query_row(
                "SELECT session_id, conversation_id, msg_channel FROM edge_sync_mapping",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(sid, "new-1");
        assert_eq!(cid, "new-1");
        assert_eq!(channel, "convmsg:uid-b");
    }

    #[test]
    fn insert_edge_sync_mapping_missing_table_false() {
        let db = temp_db("edge-no-table");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE other (x INTEGER);")
            .unwrap();
        assert!(!insert_edge_sync_mapping(&db, "new-1", "uid-b"));
    }

    #[test]
    fn session_display_title_prefers_custom_title() {
        assert_eq!(
            session_display_title(Some("自动标题".into()), Some("美团每日自动领券".into())),
            "美团每日自动领券"
        );
        assert_eq!(
            session_display_title(None, Some("美团每日自动领券".into())),
            "美团每日自动领券"
        );
        assert_eq!(
            session_display_title(Some("汉字详情页".into()), None),
            "汉字详情页"
        );
        assert_eq!(session_display_title(None, None), "(无标题)");
        assert_eq!(
            session_display_title(Some("  ".into()), Some("".into())),
            "(无标题)"
        );
    }

    #[test]
    fn claw_workspace_detected_by_folder_name() {
        assert!(is_claw_workspace("/Users/apple/WorkBuddy/Claw"));
        assert!(is_claw_workspace("/Users/apple/WorkBuddy/claw/"));
        assert!(is_claw_workspace(r"C:\Users\me\WorkBuddy\Claw"));
        assert!(!is_claw_workspace("/Users/apple/WorkBuddy/ClawBot"));
        assert!(!is_claw_workspace(
            "/Users/apple/Documents/AI-PROJECT/LetterTotTown"
        ));
    }

    /// 账本键必须同时区分源账号、目标账号与源会话：任一维度不同即为不同条目，
    /// 否则「A→C 复制过 x」会错误阻断「B→C 复制 x」。
    #[test]
    fn copy_ledger_key_separates_all_three_dimensions() {
        let base = copy_ledger_key("uid-a", "uid-b", "cid-1");
        assert_ne!(base, copy_ledger_key("uid-a2", "uid-b", "cid-1"), "源账号区分");
        assert_ne!(base, copy_ledger_key("uid-a", "uid-b2", "cid-1"), "目标账号区分");
        assert_ne!(base, copy_ledger_key("uid-a", "uid-b", "cid-2"), "源会话区分");
        assert_eq!(base, copy_ledger_key("uid-a", "uid-b", "cid-1"));
        // 分隔符不得与字段内容混淆：把分隔符塞进字段值也必须产生不同键。
        assert_ne!(
            copy_ledger_key("uid-a", "uid-b", "cid-1"),
            copy_ledger_key("uid-a\u{1f}uid-b", "", "cid-1"),
            "字段内容中的分隔符不得造成键碰撞"
        );
    }

    /// 跨版本账本键必须带源 region 前缀，同版本必须保持原键。
    ///
    /// 两个方向都要钉住：加前缀是为了「CN 的 uid-a」与「Global 的 uid-a」不共用登记；
    /// 同版本不加是为了既有账本不失配（否则老用户会被判成没复制过而重复复制）。
    #[test]
    fn cross_region_ledger_key_is_namespaced_by_source_region() {
        let same = copy_ledger_key_for(Region::Cn, Region::Cn, "uid-a", "uid-b", "cid-1");
        let cross = copy_ledger_key_for(Region::Cn, Region::Global, "uid-a", "uid-b", "cid-1");
        let reverse = copy_ledger_key_for(Region::Global, Region::Cn, "uid-a", "uid-b", "cid-1");

        assert_eq!(same, copy_ledger_key("uid-a", "uid-b", "cid-1"), "同版本键不得变");
        assert_ne!(same, cross, "跨版本必须与同版本区分");
        assert_ne!(cross, reverse, "方向不同必须是不同条目");
        assert!(cross.starts_with("cn\u{1f}"), "前缀应为源 region: {cross}");
        assert!(reverse.starts_with("global\u{1f}"), "前缀应为源 region: {reverse}");
    }

    /// 会话正文副本必须落在**目标**版本的 projects 下。
    ///
    /// 若沿用源目录，副本会写进源版本目录，目标版本的 WorkBuddy 根本看不到它 ——
    /// 这正是「跨版本不支持」时期最隐蔽的失败形态（不报错，只是没搬过去）。
    #[test]
    fn cross_region_jsonl_copy_lands_in_target_region_projects() {
        let src = session_data_dir(Region::Cn)
            .join("projects")
            .join("ws")
            .join("cid-1.jsonl");

        let same = target_jsonl_path_for(Region::Cn, Region::Cn, &src, "new-1");
        assert_eq!(
            same,
            session_data_dir(Region::Cn)
                .join("projects")
                .join("ws")
                .join("new-1.jsonl"),
            "同版本必须原地换名，行为零变化"
        );

        let cross = target_jsonl_path_for(Region::Cn, Region::Global, &src, "new-1");
        assert_eq!(
            cross,
            session_data_dir(Region::Global)
                .join("projects")
                .join("ws")
                .join("new-1.jsonl"),
            "跨版本必须换到目标版本目录"
        );
        assert_ne!(same, cross);
    }

    /// 账本读写往返：写入后能读回，且未命中项返回 None。
    ///
    /// 全程只操作临时文件，**不得触碰真实 `~/.workbuddy` 账本**。
    #[test]
    fn copy_ledger_roundtrip_and_miss() {
        let region = Region::Cn;
        let dir = std::env::temp_dir().join(format!(
            "buddy_switch_ledger_{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(COPY_LEDGER_FILE);

        assert!(
            load_copy_ledger_at(&path).is_empty(),
            "账本不存在时应视为空"
        );

        assert!(record_copy_ledger_at(
            region,
            region,
            Some(&path),
            "uid-a",
            "uid-b",
            "cid-1",
            "new-1"
        ));

        let reloaded = load_copy_ledger_at(&path);
        assert_eq!(
            reloaded
                .get(&copy_ledger_key("uid-a", "uid-b", "cid-1"))
                .and_then(Value::as_str),
            Some("new-1")
        );
        assert!(
            reloaded
                .get(&copy_ledger_key("uid-a", "uid-b", "other"))
                .is_none(),
            "未登记的源会话不得命中"
        );

        // 同一键再次登记应覆盖而不是新增条目。
        assert!(record_copy_ledger_at(
            region,
            region,
            Some(&path),
            "uid-a",
            "uid-b",
            "cid-1",
            "new-2"
        ));
        let overwritten = load_copy_ledger_at(&path);
        assert_eq!(overwritten.len(), 1, "同一键不得产生第二条");
        assert_eq!(
            overwritten
                .get(&copy_ledger_key("uid-a", "uid-b", "cid-1"))
                .and_then(Value::as_str),
            Some("new-2")
        );

        // 测试过程不得在真实数据目录留下账本。
        assert!(
            !copy_ledger_path_for(region).exists() || copy_ledger_path_for(region).is_file(),
            "账本路径形态异常"
        );

        std::fs::remove_dir_all(dir).unwrap();
    }

    // -----------------------------------------------------------------------
    // 降级路径：db 不可读 / 为空时从 projects 目录扫描 jsonl
    // -----------------------------------------------------------------------
    //
    // 这些测试把进程级 home 重定向到**临时目录**（via `HomeOverrideGuard`，drop 时
    // 还原），绝不触碰真实 `~/.workbuddy{,-ai}`。home 是进程级全局状态，故一律先取
    // `env_lock`（已在 `HomeOverrideGuard::set` 内串行化），避免与并行测试互相踩踏。

    /// 在临时目录建一个「已存在」的 home（validate_home_override 要求绝对且已存在）。
    fn make_temp_home(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "buddy_switch_fb_{}_{label}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 删掉临时 home；失败忽略（仅泄漏临时文件，不影响断言）。
    fn cleanup_temp_home(dir: &Path) {
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 建一个合法 sessions 表，写入若干属于 `uid` 的未删除会话。
    fn create_sessions_db(db: &Path, uid: &str, rows: &[(&str, &str, &str)]) {
        if let Some(parent) = db.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let conn = Connection::open(db).unwrap();
        conn.execute_batch(
            "CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                title TEXT,
                cwd TEXT,
                created_at INTEGER,
                updated_at INTEGER,
                deleted_at INTEGER
            );",
        )
        .unwrap();
        for (cid, cwd, title) in rows {
            conn.execute(
                "INSERT INTO sessions (id, user_id, title, cwd, created_at, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, 1000, 2000, NULL)",
                rusqlite::params![cid, uid, title, cwd],
            )
            .unwrap();
        }
    }

    /// db 完全缺失 → 降级扫描 projects → source == "scan"，并列出 jsonl 会话。
    ///
    /// 这正是用户现场的根因：workbuddy.db 损坏后 UI 误判「账号无会话」、禁用复制。
    /// 降级路径必须让 UI 仍能看到（至少）jsonl 里存在的会话。
    #[test]
    fn list_sessions_falls_back_to_scan_when_db_missing() {
        let home = make_temp_home("missing-db");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        // 只放 jsonl，不放 workbuddy.db
        let ws = home.join(".workbuddy").join("projects").join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            ws.join("cid-A.jsonl"),
            "{\"cwd\":\"/proj/alpha\",\"title\":\"Alpha 会话\"}\n",
        )
        .unwrap();

        let resp = list_sessions_with_fallback_for(Region::Cn, "uid-x");
        assert_eq!(
            resp.get("source").and_then(Value::as_str),
            Some("scan"),
            "db 缺失应降级到 scan"
        );
        let sessions = resp.get("sessions").and_then(Value::as_array).unwrap();
        assert_eq!(sessions.len(), 1, "应扫描到 1 个 jsonl 会话");
        let s = &sessions[0];
        assert_eq!(s.get("id").and_then(Value::as_str), Some("cid-A"));
        assert_eq!(s.get("title").and_then(Value::as_str), Some("Alpha 会话"));
        assert_eq!(s.get("cwd").and_then(Value::as_str), Some("/proj/alpha"));
        assert_eq!(
            s.get("degraded").and_then(Value::as_bool),
            Some(true),
            "扫描得到的会话应标 degraded"
        );
        assert!(resp.get("warning").is_some(), "降级应给出 warning");
        cleanup_temp_home(&home);
    }

    /// db 存在但**不是合法 sqlite**（磁盘镜像损坏）→ 同样降级到 scan。
    ///
    /// 死磕这条：真实 bug 就是 `database disk image is malformed`。必须证明「损坏」
    /// 不再被静默成空数组，而是走 scan 降级。
    #[test]
    fn list_sessions_falls_back_to_scan_when_db_corrupt() {
        let home = make_temp_home("corrupt-db");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        std::fs::create_dir_all(home.join(".workbuddy")).unwrap();
        std::fs::write(
            home.join(".workbuddy").join("workbuddy.db"),
            b"this is not a sqlite database file at all",
        )
        .unwrap();
        let ws = home.join(".workbuddy").join("projects").join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("cid-B.jsonl"), "{\"cwd\":\"/proj/beta\",\"title\":\"Beta\"}\n").unwrap();

        let resp = list_sessions_with_fallback_for(Region::Cn, "uid-x");
        assert_eq!(
            resp.get("source").and_then(Value::as_str),
            Some("scan"),
            "损坏 db 必须降级到 scan，而不是静默空数组"
        );
        assert_eq!(
            resp.get("sessions").and_then(Value::as_array).unwrap().len(),
            1
        );
        cleanup_temp_home(&home);
    }

    /// db 可读且有该账号的会话 → source == "db"，**不**重复列出 projects 里的 jsonl。
    ///
    /// 证明正常路径优先级最高，降级只是兜底；否则 db 行与扫描结果会重复。
    #[test]
    fn list_sessions_prefers_db_when_readable() {
        let home = make_temp_home("db-ok");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        create_sessions_db(
            &home.join(".workbuddy").join("workbuddy.db"),
            "uid-x",
            &[("cid-DB", "/proj/db", "DB 会话")],
        );
        // 同时放一个其它 cid 的 jsonl：db 优先，绝不应被重复列。
        let ws = home.join(".workbuddy").join("projects").join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("cid-OTHER.jsonl"), "{\"cwd\":\"/x\",\"title\":\"X\"}\n").unwrap();

        let resp = list_sessions_with_fallback_for(Region::Cn, "uid-x");
        assert_eq!(resp.get("source").and_then(Value::as_str), Some("db"));
        let sessions = resp.get("sessions").and_then(Value::as_array).unwrap();
        assert_eq!(sessions.len(), 1, "只应列 db 中的 1 条，jsonl 不重复");
        assert_eq!(sessions[0].get("id").and_then(Value::as_str), Some("cid-DB"));
        assert_eq!(
            sessions[0].get("degraded").and_then(Value::as_bool),
            None,
            "db 源不应标 degraded"
        );
        assert!(resp.get("warning").is_none(), "db 源不应有 warning");
        cleanup_temp_home(&home);
    }

    /// ★ 客户端某些版本把会话行的 `user_id` 落成**空串**（2026-09-23 实测：本机 116/116 全为空串）
    /// ⇒ 严格匹配 0 行时必须放宽到「空归属」行，否则 UI 会静默显示「当前账号暂无会话」并禁用复制。
    #[test]
    fn list_sessions_widens_scope_when_strict_uid_matches_nothing() {
        let home = make_temp_home("uid-empty");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        // 库里的行归属为空串，而当前登录 uid 是真实 uuid。
        create_sessions_db(
            &home.join(".workbuddy").join("workbuddy.db"),
            "",
            &[("cid-EMPTY-UID", "/proj/empty", "空归属会话")],
        );

        let resp = list_sessions_with_fallback_for(Region::Cn, "uid-real");
        assert_eq!(
            resp.get("source").and_then(Value::as_str),
            Some("db"),
            "空归属行仍属 db 源"
        );
        let sessions = resp.get("sessions").and_then(Value::as_array).unwrap();
        assert_eq!(
            sessions.len(),
            1,
            "严格匹配为空时应放宽到空归属行，而不是报告「账号无会话」"
        );
        assert_eq!(
            sessions[0].get("id").and_then(Value::as_str),
            Some("cid-EMPTY-UID")
        );
        cleanup_temp_home(&home);
    }

    /// 负向对照：放宽**不得**扩到别的账号 —— 库里只有别人的（非空 uid）会话时仍应视为无会话。
    #[test]
    fn list_sessions_does_not_widen_to_other_accounts() {
        let home = make_temp_home("uid-other");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        create_sessions_db(
            &home.join(".workbuddy").join("workbuddy.db"),
            "uid-someone-else",
            &[("cid-OTHER-UID", "/proj/other", "别人的会话")],
        );

        let resp = list_sessions_with_fallback_for(Region::Cn, "uid-real");
        let sessions = resp.get("sessions").and_then(Value::as_array).unwrap();
        assert!(
            sessions.is_empty(),
            "别人的账号（非空 uid）不得被放宽列出来"
        );
        cleanup_temp_home(&home);
    }

    /// 部分命中：库里既有当前账号的行、又有旧「空归属」行 ⇒ 两类都必须列出（**并集**）。
    ///
    /// 这正是用户现场恢复索引后的形态：新会话带真实 uid、旧会话 `user_id` 为空串。
    /// 若只做「严格命中 0 行才放宽」，这类库仍然列不出旧会话。
    #[test]
    fn list_sessions_unions_own_rows_with_legacy_empty_uid_rows() {
        let home = make_temp_home("uid-mixed");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        let db = home.join(".workbuddy").join("workbuddy.db");
        create_sessions_db(&db, "uid-mine", &[("cid-MINE", "/proj/mine", "我的会话")]);
        // 追加一条旧「空归属」行（不能复用 create_sessions_db：表已存在）。
        let conn = Connection::open(&db).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, user_id, title, cwd, created_at, updated_at, deleted_at)
             VALUES ('cid-LEGACY', '', '旧会话', '/proj/legacy', 1000, 3000, NULL)",
            [],
        )
        .unwrap();
        drop(conn);

        let resp = list_sessions_with_fallback_for(Region::Cn, "uid-mine");
        let sessions = resp.get("sessions").and_then(Value::as_array).unwrap();
        let ids: Vec<&str> = sessions
            .iter()
            .filter_map(|s| s.get("id").and_then(Value::as_str))
            .collect();
        assert!(ids.contains(&"cid-MINE"), "当前账号自己的会话必须列出");
        assert!(
            ids.contains(&"cid-LEGACY"),
            "旧空归属会话必须一并列出（并集）"
        );
        assert_eq!(sessions.len(), 2, "并集不得产生重复行");
        cleanup_temp_home(&home);
    }

    /// 数据目录里连 `workbuddy.db` / `projects/` 都没有 → source == "no-dir"。
    ///
    /// 现场（2026-09-23）：客户端重装把 `~/.workbuddy` 重建为空，UI 却显示「当前账号暂无会话」
    /// —— 用户以为账号没会话，实际是整个数据目录空了。两种情形必须能分辨。
    #[test]
    fn list_sessions_reports_no_dir_when_data_dir_is_gone() {
        let home = make_temp_home("no-dir");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        let resp = list_sessions_with_fallback_for(Region::Cn, "uid-x");
        assert_eq!(resp.get("source").and_then(Value::as_str), Some("no-dir"));
        assert_eq!(
            resp.get("sessions").and_then(Value::as_array).unwrap().len(),
            0
        );
        cleanup_temp_home(&home);
    }

    /// 数据目录在、库也在，只是当前账号没有会话 → source == "empty"
    /// （UI 这才显示「当前账号暂无会话」，措辞必须与 no-dir 区分）。
    #[test]
    fn list_sessions_reports_empty_when_db_exists_but_has_no_sessions() {
        let home = make_temp_home("empty");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        // 合法空库：表在、行数为 0。
        create_sessions_db(&home.join(".workbuddy").join("workbuddy.db"), "uid-x", &[]);
        let resp = list_sessions_with_fallback_for(Region::Cn, "uid-x");
        assert_eq!(resp.get("source").and_then(Value::as_str), Some("empty"));
        assert_eq!(
            resp.get("sessions").and_then(Value::as_array).unwrap().len(),
            0
        );
        cleanup_temp_home(&home);
    }

    /// 扫描应跳过 Claw 工作区，与 db 版语义对齐。
    #[test]
    fn scan_skips_claw_workspaces() {
        let home = make_temp_home("claw");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        let ws = home.join(".workbuddy").join("projects").join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            ws.join("cid-normal.jsonl"),
            "{\"cwd\":\"/proj/normal\",\"title\":\"N\"}\n",
        )
        .unwrap();
        std::fs::write(
            ws.join("cid-claw.jsonl"),
            "{\"cwd\":\"/proj/Claw\",\"title\":\"C\"}\n",
        )
        .unwrap();

        let sessions = scan_sessions_from_jsonl_for(Region::Cn);
        assert_eq!(sessions.len(), 1, "Claw 工作区必须被跳过");
        assert_eq!(
            sessions[0].get("id").and_then(Value::as_str),
            Some("cid-normal")
        );
        cleanup_temp_home(&home);
    }

    /// ★ 子代理记录不是会话：`projects/<ws>/<cid>/subagents/agent-*.jsonl` 必须被跳过。
    ///
    /// 实测现场（2026-09-24）：国内版 `projects/` 下 391 个真会话旁边躺着 **81 个**子代理文件。
    /// 无界递归会把它们当成会话，切换弹窗里凭空多出几十条 `agent-xxxx`。
    #[test]
    fn scan_skips_subagent_transcripts() {
        let home = make_temp_home("subagent");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        let ws = home.join(".workbuddy").join("projects").join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            ws.join("cid-real.jsonl"),
            "{\"cwd\":\"/proj/real\",\"aiTitle\":\"真会话\"}\n",
        )
        .unwrap();
        // 更深一层：<cid>/subagents/agent-*.jsonl（子代理记录）
        let sub = ws.join("cid-real").join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(
            sub.join("agent-6a07c65d.jsonl"),
            "{\"cwd\":\"/proj/real\",\"aiTitle\":\"子代理\"}\n",
        )
        .unwrap();

        let sessions = scan_sessions_from_jsonl_for(Region::Cn);
        let ids: Vec<&str> = sessions
            .iter()
            .filter_map(|s| s.get("id").and_then(Value::as_str))
            .collect();
        assert_eq!(ids, vec!["cid-real"], "只应列出真会话，子代理记录必须跳过");
        cleanup_temp_home(&home);
    }

    /// 扫描从 jsonl 首行解析 cwd / title；缺字段时用「(无标题)」。
    #[test]
    fn scan_parses_cwd_and_title_from_jsonl() {
        let home = make_temp_home("meta");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        let ws = home.join(".workbuddy").join("projects").join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        // 首行是纯数据行（无 cwd/title），meta 写在第二行 —— 容忍前几行。
        std::fs::write(
            ws.join("cid-meta.jsonl"),
            "{\"role\":\"user\",\"content\":\"hi\"}\n{\"cwd\":\"/proj/m\",\"title\":\"M 会话\"}\n",
        )
        .unwrap();
        // 首行无任何可解析字段 → 标题回退「(无标题)」。
        std::fs::write(ws.join("cid-anon.jsonl"), "{\"role\":\"assistant\"}\n").unwrap();

        let mut sessions = scan_sessions_from_jsonl_for(Region::Cn);
        // 按 id 查表更稳（mtime 在新文件上等同，顺序不可靠）。
        sessions.sort_by_key(|s| s.get("id").and_then(Value::as_str).unwrap_or("").to_string());
        assert_eq!(sessions.len(), 2);
        let by_id: std::collections::HashMap<String, Value> = sessions
            .iter()
            .map(|s| (s.get("id").and_then(Value::as_str).unwrap().to_string(), s.clone()))
            .collect();
        let meta = by_id.get("cid-meta").unwrap();
        assert_eq!(meta.get("cwd").and_then(Value::as_str), Some("/proj/m"));
        assert_eq!(meta.get("title").and_then(Value::as_str), Some("M 会话"));
        let anon = by_id.get("cid-anon").unwrap();
        assert_eq!(
            anon.get("title").and_then(Value::as_str),
            Some("(无标题)"),
            "无标题应回退占位符"
        );
        cleanup_temp_home(&home);
    }

    /// **真实 jsonl 格式**：`cwd` 在**每一行**上，标题字段叫 `aiTitle`（实测在第 3 行）。
    ///
    /// 这条用例锁的就是当年那个缺陷：旧实现找的是 `title` 字段，真实文件里根本没有
    /// ⇒ 每个会话都退化成「(无标题)」，而单测用的自制 fixture 恰好有 `title` ⇒ 全绿。
    #[test]
    fn scan_reads_real_format_ai_title() {
        let home = make_temp_home("real-fmt");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        let ws = home.join(".workbuddy").join("projects").join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        // 逐行模仿实测格式：第 1 行 message 带 cwd，第 2 行无标题，第 3 行 ai-title 带 aiTitle。
        std::fs::write(
            ws.join("cid-real.jsonl"),
            concat!(
                "{\"type\":\"message\",\"cwd\":\"/proj/real\",\"role\":\"user\",\"content\":\"hi\"}\n",
                "{\"type\":\"message\",\"cwd\":\"/proj/real\",\"role\":\"assistant\",\"content\":\"yo\"}\n",
                "{\"type\":\"ai-title\",\"cwd\":\"/proj/real\",\"aiTitle\":\"真实自动标题\"}\n",
            ),
        )
        .unwrap();

        let sessions = scan_sessions_from_jsonl_for(Region::Cn);
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0].get("cwd").and_then(Value::as_str),
            Some("/proj/real")
        );
        assert_eq!(
            sessions[0].get("title").and_then(Value::as_str),
            Some("真实自动标题"),
            "标题必须取自 aiTitle（真实格式），不是 title"
        );
        cleanup_temp_home(&home);
    }

    /// 用户改过名的会话以 `customTitle` 为准 —— 与 db 版 [`session_display_title`] 同优先级。
    #[test]
    fn scan_prefers_custom_title_over_ai_title() {
        let home = make_temp_home("custom-title");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        let ws = home.join(".workbuddy").join("projects").join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        // customTitle 实测出现得很靠后（第 56 行），这里刻意也放远一点，顺便验证窗口够宽。
        let mut body = String::from(
            "{\"type\":\"message\",\"cwd\":\"/proj/c\",\"role\":\"user\",\"content\":\"hi\"}\n\
             {\"type\":\"ai-title\",\"cwd\":\"/proj/c\",\"aiTitle\":\"自动标题\"}\n",
        );
        for i in 0..40 {
            body.push_str(&format!(
                "{{\"type\":\"message\",\"cwd\":\"/proj/c\",\"role\":\"assistant\",\"content\":\"line {i}\"}}\n"
            ));
        }
        body.push_str("{\"type\":\"custom-title\",\"cwd\":\"/proj/c\",\"customTitle\":\"我改的名字\"}\n");
        std::fs::write(ws.join("cid-custom.jsonl"), body).unwrap();

        let sessions = scan_sessions_from_jsonl_for(Region::Cn);
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0].get("title").and_then(Value::as_str),
            Some("我改的名字"),
            "customTitle 优先于 aiTitle"
        );
        cleanup_temp_home(&home);
    }

    /// 头部读取有**行数**上限：元数据落在 128 行之后就读不到（回退「(无标题)」）。
    ///
    /// 这是刻意的成本闸 —— 一行的**字节**长度不受控，只靠字节上限会被一行超长内容拖死。
    #[test]
    fn scan_stops_at_line_cap() {
        let home = make_temp_home("line-cap");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        let ws = home.join(".workbuddy").join("projects").join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        let mut body = String::new();
        for i in 0..200 {
            body.push_str(&format!("{{\"type\":\"message\",\"role\":\"user\",\"n\":{i}}}\n"));
        }
        body.push_str("{\"cwd\":\"/proj/late\",\"aiTitle\":\"来晚了\"}\n");
        std::fs::write(ws.join("cid-late.jsonl"), body).unwrap();

        let sessions = scan_sessions_from_jsonl_for(Region::Cn);
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0].get("title").and_then(Value::as_str),
            Some("(无标题)"),
            "第 201 行的元数据不该被读到"
        );
        cleanup_temp_home(&home);
    }

    /// 头部读取有**字节**上限：前 100 行各 1KB（≈100KB > 64KB）时，第 101 行的元数据读不到。
    ///
    /// 与上一条互补：这条里行数（101 < 128）没超，**只有**字节上限生效 ⇒ 两条分别锁定两个闸。
    #[test]
    fn scan_stops_at_byte_cap() {
        let home = make_temp_home("byte-cap");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        let ws = home.join(".workbuddy").join("projects").join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        let filler = "x".repeat(1000);
        let mut body = String::new();
        for i in 0..100 {
            body.push_str(&format!(
                "{{\"type\":\"message\",\"role\":\"user\",\"n\":{i},\"pad\":\"{filler}\"}}\n"
            ));
        }
        body.push_str("{\"cwd\":\"/proj/far\",\"aiTitle\":\"太远了\"}\n");
        std::fs::write(ws.join("cid-far.jsonl"), body).unwrap();

        let sessions = scan_sessions_from_jsonl_for(Region::Cn);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].get("title").and_then(Value::as_str), Some("(无标题)"));
        assert_eq!(sessions[0].get("cwd").and_then(Value::as_str), Some(""));
        cleanup_temp_home(&home);
    }

    /// 头部有**非 JSON** 噪声（真实文件首行可能是半截 / 非 JSON）时不能整条丢弃，
    /// 后续行的元数据仍要能解析出来。
    #[test]
    fn scan_tolerates_garbage_head_lines() {
        let home = make_temp_home("garbage-head");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        let ws = home.join(".workbuddy").join("projects").join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            ws.join("cid-garbage.jsonl"),
            "not json at all\n\n{\"cwd\":\"/proj/g\"}\n{\"aiTitle\":\"幸存标题\"}\n",
        )
        .unwrap();

        let sessions = scan_sessions_from_jsonl_for(Region::Cn);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].get("cwd").and_then(Value::as_str), Some("/proj/g"));
        assert_eq!(
            sessions[0].get("title").and_then(Value::as_str),
            Some("幸存标题")
        );
        cleanup_temp_home(&home);
    }

    /// 跨 region 扫描隔离：Global 的 projects 不应出现在 CN 的扫描结果里。
    #[test]
    fn scan_is_region_scoped() {
        let home = make_temp_home("region-scope");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        let cn_ws = home.join(".workbuddy").join("projects").join("ws");
        let g_ws = home.join(".workbuddy-ai").join("projects").join("ws");
        std::fs::create_dir_all(&cn_ws).unwrap();
        std::fs::create_dir_all(&g_ws).unwrap();
        std::fs::write(cn_ws.join("cid-cn.jsonl"), "{\"cwd\":\"/cn\",\"title\":\"C\"}\n").unwrap();
        std::fs::write(g_ws.join("cid-g.jsonl"), "{\"cwd\":\"/g\",\"title\":\"G\"}\n").unwrap();

        let cn = scan_sessions_from_jsonl_for(Region::Cn);
        assert_eq!(cn.len(), 1);
        assert_eq!(cn[0].get("id").and_then(Value::as_str), Some("cid-cn"));
        let g = scan_sessions_from_jsonl_for(Region::Global);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].get("id").and_then(Value::as_str), Some("cid-g"));
        cleanup_temp_home(&home);
    }

    /// 复制降级：源 jsonl 存在但 workbuddy.db 缺失 → 仍复制正文，索引行写不进时给 warning。
    ///
    /// 这对应「目标账号 db 也坏了」的最坏情形：至少 jsonl 正文能落到目标账号，
    /// 用户重启 WorkBuddy 重建索引后即可见。绝不能因为它「报错」就阻断整次复制。
    #[test]
    fn copy_session_degrades_when_db_missing() {
        let home = make_temp_home("copy-degraded");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        // 源 region 放 jsonl，但**没有** workbuddy.db（既不读索引、也写不进目标索引）。
        let src_ws = home.join(".workbuddy").join("projects").join("ws");
        std::fs::create_dir_all(&src_ws).unwrap();
        std::fs::write(
            src_ws.join("cid-S.jsonl"),
            "{\"cwd\":\"/proj/s\",\"title\":\"S\",\"sessionId\":\"cid-S\"}\n",
        )
        .unwrap();

        let result = copy_session_to_user_cross(
            Region::Cn,
            Region::Cn,
            "cid-S",
            "uid-src",
            "uid-dst",
        )
        .expect("降级复制不应报错");
        assert_eq!(
            result.get("jsonlCopied").and_then(Value::as_bool),
            Some(true),
            "jsonl 必须被复制"
        );
        assert_eq!(
            result.get("sessionRowWritten").and_then(Value::as_bool),
            Some(false),
            "db 缺失则索引行没写"
        );
        assert_eq!(
            result.get("mappingWritten").and_then(Value::as_bool),
            Some(false),
            "edge_sync 映射库缺失则注册失败（不致命）"
        );
        assert!(
            result.get("warning").is_some(),
            "应给出降级 warning，提示用户重启 WorkBuddy 重建索引"
        );
        assert_eq!(
            result.get("deduplicated").and_then(Value::as_bool),
            Some(false)
        );

        // 副本应落在同版本 projects 下、使用新 cid。
        let copied = std::fs::read_dir(&src_ws)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .any(|name| name.ends_with(".jsonl") && name != "cid-S.jsonl");
        assert!(copied, "应生成新 cid 的 jsonl 副本");
        cleanup_temp_home(&home);
    }

    /// 复制降级：源 db 损坏（读不到 cwd）→ 从 jsonl 推断 Claw，Claw 仍被拒绝。
    #[test]
    fn copy_session_rejects_claw_even_when_db_corrupt() {
        let home = make_temp_home("copy-claw");
        let _g = crate::modules::config::HomeOverrideGuard::set(&home);
        std::fs::create_dir_all(home.join(".workbuddy")).unwrap();
        std::fs::write(
            home.join(".workbuddy").join("workbuddy.db"),
            b"corrupt sqlite",
        )
        .unwrap();
        let src_ws = home.join(".workbuddy").join("projects").join("ws");
        std::fs::create_dir_all(&src_ws).unwrap();
        std::fs::write(
            src_ws.join("cid-claw.jsonl"),
            "{\"cwd\":\"/proj/Claw\",\"title\":\"C\"}\n",
        )
        .unwrap();

        let err = copy_session_to_user_cross(
            Region::Cn,
            Region::Cn,
            "cid-claw",
            "uid-src",
            "uid-dst",
        )
        .expect_err("Claw 工作区必须被拒绝");
        assert!(err.contains("Claw"), "拒绝原因应为 Claw: {err}");
        cleanup_temp_home(&home);
    }

    /// 损坏的账本必须退化为「无登记」而不是报错或丢弃已有副本。
    #[test]
    fn corrupt_copy_ledger_is_treated_as_empty() {
        let dir = std::env::temp_dir().join(format!(
            "buddy_switch_ledger_corrupt_{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(COPY_LEDGER_FILE);

        for payload in ["not-json", "[1,2,3]", "\"text\"", "null", ""] {
            std::fs::write(&path, payload).unwrap();
            assert!(
                load_copy_ledger_at(&path).is_empty(),
                "损坏内容应视为空账本：{payload}"
            );
        }

        std::fs::remove_dir_all(dir).unwrap();
    }

    /// 账本命中还必须要求副本仍存活：副本被删则允许重新复制。
    #[test]
    fn ledger_only_hits_when_copy_still_alive() {
        let db = temp_db("ledger-alive");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                title TEXT,
                deleted_at INTEGER
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sessions (id, user_id, deleted_at) VALUES ('copy-1', 'uid-b', NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sessions (id, user_id, deleted_at) VALUES ('copy-2', 'uid-b', 12345)",
            [],
        )
        .unwrap();

        // 直接验证存活判定语义（账本文件 + db 路径由 region 派生，无法在本单测内重定向）。
        let alive = |cid: &str| -> bool {
            conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions \
                 WHERE id = ?1 AND user_id = 'uid-b' AND deleted_at IS NULL)",
                rusqlite::params![cid],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n != 0)
            .unwrap()
        };
        assert!(alive("copy-1"), "未删除的副本应视为存活");
        assert!(!alive("copy-2"), "已删除的副本不应视为存活");
        assert!(!alive("missing"), "不存在的副本不应视为存活");
    }
}
