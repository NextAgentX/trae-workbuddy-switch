//! 跨版本（CN ↔ Global）数据搬运的**可证伪**集成测试。
//!
//! ## 为什么需要它
//!
//! `*_cross(source_region, …, target_region, …)` 三个入口（会话 / 记忆 / 连接器）在
//! 单测里只能被钉住「路径与键名算得对不对」，钉不住**真的读到了源版本、真的只写了目标版本**。
//! 而后者正是跨版本最危险的失败形态：不报错，只是数据没过去 ——
//! 用户会以为搬完了，切过去发现会话/记忆全无。
//!
//! ## 隔离方式
//!
//! 与 `switch_region_binding.rs` 同源：临时目录 + `BUDDY_SWITCH_HOME`，并用
//! [`ENV_LOCK`] 串行化（环境变量是进程级全局状态）。
//!
//! ## 可证伪性
//!
//! 若生产代码把「源 region」误写成「目标 region」：
//! - 会话用例会因源 uid 取到 `uid-global-dst`（而不是 `uid-cn-src`）→ 源库里查不到该会话
//!   → `copied` 为空，断言 `copied.len() == 1` 直接红；
//! - 记忆用例的 `appended` 会是 0（源记忆文件根本不存在）→ 断言红。
//! 若把「目标 region」误写成「源 region」，则正文会写回 CN 的 projects、
//! 副本进不了 Global 库 —— 落点断言与「源库不得被写入」断言同时红。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use serde_json::json;

use buddy_switch_core::modules::config::BUDDY_SWITCH_HOME_ENV;
use buddy_switch_core::modules::region::Region;
use buddy_switch_core::modules::{config, memory, migrate, session};

/// 串行化所有会修改 `BUDDY_SWITCH_HOME` 的测试（该变量是进程级全局状态）。
static ENV_LOCK: Mutex<()> = Mutex::new(());

const CN_UID: &str = "uid-cn-src";
const GLOBAL_UID: &str = "uid-global-dst";
const CID: &str = "11111111-2222-3333-4444-555555555555";

/// 认证文件相对 home 的平台结构（与 `auth_file` 独立编码，避免依赖被测函数自身）。
fn auth_rel(filename: &str) -> PathBuf {
    #[cfg(target_os = "windows")]
    let dir = "AppData/Local/CodeBuddyExtension/Data/Public/auth";
    #[cfg(target_os = "macos")]
    let dir = "Library/Application Support/CodeBuddyExtension/Data/Public/auth";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let dir = ".local/share/CodeBuddyExtension/Data/Public/auth";

    Path::new(dir).join(filename)
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("buddy-switch-{label}-{}-{nanos}", std::process::id()))
}

/// 隔离 home 守卫：设置 `BUDDY_SWITCH_HOME`、持有 [`ENV_LOCK`]，Drop 时恢复并清理。
struct IsolatedHome {
    _lock: MutexGuard<'static, ()>,
    previous: Option<String>,
    dir: PathBuf,
}

impl IsolatedHome {
    fn new(label: &str) -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::env::var(BUDDY_SWITCH_HOME_ENV).ok();
        let dir = unique_temp_dir(label);
        fs::create_dir_all(&dir).expect("create isolated home");
        std::env::set_var(BUDDY_SWITCH_HOME_ENV, &dir);
        Self {
            _lock: lock,
            previous,
            dir,
        }
    }

    fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for IsolatedHome {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var(BUDDY_SWITCH_HOME_ENV, value),
            None => std::env::remove_var(BUDDY_SWITCH_HOME_ENV),
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn write_auth(home: &Path, filename: &str, uid: &str) {
    let path = home.join(auth_rel(filename));
    fs::create_dir_all(path.parent().expect("auth parent")).expect("create auth dir");
    fs::write(
        path,
        format!(r#"{{"account":{{"uid":"{uid}"}},"auth":{{"accessToken":"X"}}}}"#),
    )
    .expect("write auth file");
}

/// 建一个带 `sessions` 表的 workbuddy.db；表结构刻意比生产少几列，
/// 以证明复制走的是「动态列」而不是硬编码列清单。
fn create_sessions_db(path: &Path) -> Connection {
    fs::create_dir_all(path.parent().expect("db parent")).expect("create data dir");
    let conn = Connection::open(path).expect("open db");
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            title TEXT,
            cwd TEXT,
            created_at INTEGER,
            updated_at INTEGER,
            deleted_at INTEGER
         );",
    )
    .expect("create sessions table");
    conn
}

/// 跨版本复制会话：源读 CN、只写 Global，且跨版本账本键生效（二次复制被跳过）。
#[test]
fn cross_region_session_copy_writes_only_into_target_region() {
    let home = IsolatedHome::new("cross-session");
    let h = home.path();
    assert_eq!(config::home_dir(), h.to_path_buf());

    write_auth(h, "workbuddy-desktop.info", CN_UID);
    write_auth(h, "workbuddy-desktop-ai.info", GLOBAL_UID);

    // ---- 源：CN ----
    let cn_db = h.join(".workbuddy").join("workbuddy.db");
    let cn_conn = create_sessions_db(&cn_db);
    cn_conn
        .execute(
            "INSERT INTO sessions (id, user_id, title, cwd, created_at, updated_at, deleted_at)
             VALUES (?1, ?2, '跨版本会话', 'D:/ws', 1, 2, NULL)",
            rusqlite::params![CID, CN_UID],
        )
        .expect("insert source session");
    drop(cn_conn);

    let cn_ws = h.join(".workbuddy").join("projects").join("ws");
    fs::create_dir_all(&cn_ws).expect("create cn projects");
    fs::write(
        cn_ws.join(format!("{CID}.jsonl")),
        format!("{{\"sessionId\":\"{CID}\",\"text\":\"来自国内版\"}}\n"),
    )
    .expect("write source jsonl");

    // ---- 目标：Global（空会话库 + 边车映射库）----
    let global_db = h.join(".workbuddy-ai").join("workbuddy.db");
    drop(create_sessions_db(&global_db));
    // 档位号用**当前客户端**的 v4（真机 2026-09-24 实测：v2/v3 已迁走、只剩 v4）。
    // ★ 这条断言是可证伪的：若 `edge_sync_db_path_for` 退回写死 v2，映射会被写进
    //   一个新建的空 v2 库、`insert_edge_sync_mapping` 返回 false，下面那条
    //   「mapping row」查询在 v4 里查不到行 ⇒ 本用例红。
    let edge_db = h.join(".workbuddy-ai").join("edge-sync-mapping-v4.db");
    fs::create_dir_all(edge_db.parent().expect("edge parent")).expect("create edge dir");
    let edge = Connection::open(&edge_db).expect("open edge db");
    edge.execute_batch(
        "CREATE TABLE IF NOT EXISTS edge_sync_mapping (
            session_id TEXT,
            conversation_id TEXT,
            msg_channel TEXT,
            created_at INTEGER
         );",
    )
    .expect("create mapping table");
    drop(edge);

    // ---- 执行 ----
    let target = json!({ "uid": GLOBAL_UID });
    let report = session::copy_sessions_for_switch_cross(
        Region::Cn,
        Region::Global,
        &target,
        &[CID.to_string()],
    )
    .expect("跨版本复制应产出报告");

    assert_eq!(report["sourceUid"], CN_UID, "源 uid 必须取自**源**版本");
    assert_eq!(report["targetUid"], GLOBAL_UID);
    assert_eq!(report["sourceRegion"], "cn");
    assert_eq!(report["targetRegion"], "global");

    let copied = report["copied"].as_array().expect("copied array");
    assert_eq!(copied.len(), 1, "应复制 1 个会话：{report}");
    let new_id = copied[0]["newId"].as_str().expect("newId").to_string();
    assert_ne!(new_id, CID, "副本必须是新 id");
    assert_eq!(copied[0]["jsonlCopied"], true, "正文应被复制");

    // 副本落在目标库且归属目标账号
    let g = Connection::open(&global_db).expect("open global db");
    let (owner, title): (String, String) = g
        .query_row(
            "SELECT user_id, title FROM sessions WHERE id = ?1",
            [&new_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("copy row must exist in target region");
    assert_eq!(owner, GLOBAL_UID, "副本必须归属目标账号");
    assert_eq!(title, "跨版本会话", "正文列应原样保留");

    // 源库不得被写入
    let s = Connection::open(&cn_db).expect("open cn db");
    let src_rows: i64 = s
        .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
        .expect("count source rows");
    assert_eq!(src_rows, 1, "源库不得被写入");
    drop(s);

    // 正文必须落在**目标**版本的 projects
    let dst_jsonl = h
        .join(".workbuddy-ai")
        .join("projects")
        .join("ws")
        .join(format!("{new_id}.jsonl"));
    assert!(
        dst_jsonl.is_file(),
        "正文必须落到目标版本 projects：{}",
        dst_jsonl.display()
    );
    let text = fs::read_to_string(&dst_jsonl).expect("read copy jsonl");
    assert!(text.contains(&new_id), "副本正文里的 id 必须已替换：{text}");
    assert!(!text.contains(CID), "不得残留源会话 id：{text}");

    // 云端映射归属目标账号
    let e = Connection::open(&edge_db).expect("open edge db");
    let channel: String = e
        .query_row(
            "SELECT msg_channel FROM edge_sync_mapping WHERE session_id = ?1",
            [&new_id],
            |r| r.get(0),
        )
        .expect("mapping row");
    assert_eq!(channel, format!("convmsg:{GLOBAL_UID}"));

    // 账本写在目标侧，且键带源 region 前缀
    let ledger_path = h
        .join(".workbuddy-ai")
        .join("buddy-switch-copy-ledger.json");
    assert!(
        ledger_path.is_file(),
        "账本必须落在目标版本目录：{}",
        ledger_path.display()
    );
    let ledger_text = fs::read_to_string(&ledger_path).expect("read ledger");
    // 注意：serde_json 把分隔符 U+001F 转义成 `\u001f` 写入文件，故按转义后的文本比对。
    let expected_key = format!("\"cn\\u001f{CN_UID}\\u001f{GLOBAL_UID}\\u001f{CID}\"");
    assert!(
        ledger_text.contains(&expected_key),
        "跨版本账本键必须带源 region 前缀：{ledger_text}"
    );

    // 二次复制必须命中去重 —— 证明跨版本键真被读到了
    let again = session::copy_sessions_for_switch_cross(
        Region::Cn,
        Region::Global,
        &target,
        &[CID.to_string()],
    )
    .expect("二次复制");
    assert_eq!(
        again["copied"].as_array().expect("copied").len(),
        0,
        "重复复制不得产生新副本"
    );
    let skipped = again["skipped"].as_array().expect("skipped");
    assert_eq!(skipped.len(), 1, "必须归类为「已存在，跳过」：{again}");
}

/// 跨版本迁移记忆与连接器：源只读、只写目标、去重生效、二次幂等。
#[test]
fn cross_region_memory_and_connectors_merge_into_target_region() {
    let home = IsolatedHome::new("cross-migrate");
    let h = home.path();

    write_auth(h, "workbuddy-desktop.info", CN_UID);
    write_auth(h, "workbuddy-desktop-ai.info", GLOBAL_UID);

    // 源记忆（CN）：两行，其中「语言：中文」与目标重复
    let cn_mem_dir = memory::memory_dir_for(Region::Cn);
    fs::create_dir_all(&cn_mem_dir).expect("create cn memory dir");
    let cn_mem_file = memory::memory_file_for(Region::Cn, CN_UID);
    fs::write(&cn_mem_file, "- 偏好：表格展示\n- 语言：中文\n").expect("write cn memory");

    // 目标记忆（Global）：一行
    let g_mem_dir = memory::memory_dir_for(Region::Global);
    fs::create_dir_all(&g_mem_dir).expect("create global memory dir");
    let g_mem_file = memory::memory_file_for(Region::Global, GLOBAL_UID);
    fs::write(&g_mem_file, "- 语言：中文\n").expect("write global memory");

    // 连接器：源多一个 `a`，目标数组已有 1
    let cn_conn_dir = h.join(".workbuddy").join("connectors").join(CN_UID);
    fs::create_dir_all(&cn_conn_dir).expect("create cn connector dir");
    fs::write(
        cn_conn_dir.join("mcp.json"),
        r#"{"mcpServers":{"a":{"command":"node a.js"}},"list":[1,2]}"#,
    )
    .expect("write cn mcp.json");

    let g_conn_dir = h.join(".workbuddy-ai").join("connectors").join(GLOBAL_UID);
    fs::create_dir_all(&g_conn_dir).expect("create global connector dir");
    fs::write(
        g_conn_dir.join("mcp.json"),
        r#"{"mcpServers":{},"list":[1]}"#,
    )
    .expect("write global mcp.json");

    let cn_mem_before = fs::read_to_string(&cn_mem_file).expect("read cn memory");
    let cn_mcp_before = fs::read_to_string(cn_conn_dir.join("mcp.json")).expect("read cn mcp");

    // ---- 执行 ----
    let report = migrate::migrate_account_data_cross(
        Region::Cn,
        CN_UID,
        Region::Global,
        GLOBAL_UID,
        migrate::MigrateScope::default(),
    )
    .expect("跨版本迁移应成功");

    assert_eq!(report["sourceRegion"], "cn");
    assert_eq!(report["targetRegion"], "global");
    assert_eq!(report["changed"], true, "应有实际改写：{report}");
    assert_eq!(report["memory"]["changed"], true);
    assert_eq!(
        report["memory"]["appended"], 1,
        "只补差集：重复行不得重复追加：{report}"
    );
    assert_eq!(report["connectors"]["changed"], true);

    // 目标记忆：新增了源独有行，且重复行没有变成两行
    let merged = fs::read_to_string(&g_mem_file).expect("read merged memory");
    assert!(
        merged.contains("偏好：表格展示"),
        "目标记忆应含源独有行：{merged}"
    );
    assert_eq!(
        merged.matches("语言：中文").count(),
        1,
        "重复行必须被去重：{merged}"
    );

    // 目标连接器：源的新键合并进来、数组按元素去重
    let mcp: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(g_conn_dir.join("mcp.json")).expect("read mcp"))
            .expect("parse merged mcp.json");
    assert!(
        mcp["mcpServers"].get("a").is_some(),
        "连接器的新条目必须进目标版本：{mcp}"
    );
    let list = mcp["list"].as_array().expect("list array");
    assert_eq!(list.len(), 2, "数组应做并集去重：[1] + [1,2] → [1,2]：{mcp}");

    // 源侧只读
    assert_eq!(
        fs::read_to_string(&cn_mem_file).expect("read cn memory"),
        cn_mem_before,
        "源记忆不得被改写"
    );
    assert_eq!(
        fs::read_to_string(cn_conn_dir.join("mcp.json")).expect("read cn mcp"),
        cn_mcp_before,
        "源连接器不得被改写"
    );

    // 二次迁移必须幂等（不产生新内容）
    let second = migrate::migrate_account_data_cross(
        Region::Cn,
        CN_UID,
        Region::Global,
        GLOBAL_UID,
        migrate::MigrateScope::default(),
    )
    .expect("二次迁移");
    assert_eq!(second["changed"], false, "二次迁移必须幂等：{second}");
}
