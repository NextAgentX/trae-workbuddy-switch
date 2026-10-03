//! 对话转发抓取（诊断旁路）。
//!
//! ## 用途
//!
//! 每次出站尝试落盘一组文件（目录 `~/.buddy-switch/trae/capture/`）：
//!
//! | 后缀 | 内容 | 何时写入 |
//! |:---|:---|:---|
//! | `_client.json` | 客户端发来的**原始请求体**（OpenAI/Anthropic 原样） | 出站前 |
//! | `_upstream.json` | 改写后发往上游的 `llm_utils_chat` 请求体 | 出站前 |
//! | `_upstream_resp.ndjson` | 上游返回的**原始 SSE 流**（逐块追加） | 流式消费时 |
//! | `_upstream_http_error.txt` | 上游非 2xx 的错误响应体 | 连接期失败时 |
//!
//! 编号（文件名前缀）是"一次出站尝试"的关联键：换号重试时每次尝试各自
//! 一组文件，同一次尝试的请求对与响应流靠编号配对。
//!
//! ## 为什么失败也静默
//!
//! 抓取是诊断旁路：落盘失败（磁盘满 / 权限 / 目录被删）绝不能影响转发主链路，
//! 顶多少一份诊断材料。因此所有 IO 错误一律忽略，不记日志、不上抛。

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use buddy_switch_core::modules::trae::paths;

/// 尝试序号（进程内自增，避免同毫秒内编号碰撞）。
static SEQ: AtomicU64 = AtomicU64::new(0);

/// 测试专用抓取根目录覆盖：`Some(path)` 时全部抓取落到该目录。
///
/// 为什么不用环境变量：并发测试对 home 的覆盖是**进程级**环境变量，
/// 多线程下互相污染（A 线程写入 B 线程的临时目录）——真实发生过
/// （apikey 系测试把 home 指到各自 `Temp\trae-apikey-*`）。
/// 模块内覆盖与 home 解析彻底解耦，产品代码永不设置，因此零行为影响。
static TEST_ROOT: RwLock<Option<PathBuf>> = RwLock::new(None);

/// 抓取根目录：`~/.buddy-switch/trae/capture/`（不存在则创建）。
///
/// 复用 Trae 数据目录机制（[`paths::trae_dir`]）而不是相对工作目录的 `temp/`：
/// 网关进程的工作目录不确定（Tauri 桌面端 / webui / 测试各不相同），
/// 只有数据目录的解析是稳定可预期的。测试可用 [`set_test_root`] 重定向。
pub fn capture_dir() -> PathBuf {
    let dir = TEST_ROOT
        .read()
        .unwrap()
        .clone()
        .unwrap_or_else(|| paths::trae_dir().join("capture"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// 仅供测试：设置/清除抓取根目录覆盖（`None` 恢复默认数据目录）。
#[cfg(test)]
pub fn set_test_root(dir: Option<PathBuf>) {
    *TEST_ROOT.write().unwrap() = dir;
}

/// 生成本次出站尝试的抓取编号：`<yyyyMMdd_HHmmss>_<毫秒>_<序号3位>`。
///
/// 编号同时是文件名前缀与请求/响应的配对键；序号取模 1000 只用于
/// 同毫秒去重，不承担别的语义。
pub fn new_capture_id() -> String {
    let seq = SEQ.fetch_add(1, Ordering::Relaxed) % 1000;
    let now = chrono::Local::now();
    format!(
        "{}_{:03}_{:03}",
        now.format("%Y%m%d_%H%M%S"),
        now.timestamp_subsec_millis(),
        seq
    )
}

/// 写一个抓取文件（整文件覆盖写，用于请求体 / 错误体）。
///
/// 空 `capture_id` 表示本次调用未启用抓取（开关关闭或单元测试），直接跳过。
pub fn write_capture(capture_id: &str, suffix: &str, content: &[u8]) {
    if capture_id.is_empty() {
        return;
    }
    let path = capture_dir().join(format!("{capture_id}{suffix}"));
    let _ = std::fs::write(path, content);
}

/// 追加一个抓取文件（用于响应流逐块落盘）。
///
/// 与 [`write_capture`] 同样静默：追加失败不重试、不报错。
pub fn append_capture(capture_id: &str, suffix: &str, content: &[u8]) {
    if capture_id.is_empty() {
        return;
    }
    let path = capture_dir().join(format!("{capture_id}{suffix}"));
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(content);
    }
}

/// 抓一次出站尝试的**请求对**：客户端原文体 + 改写后上游体。
///
/// 两个方向各一个文件，编号相同；调用点在 [`super::routes`] 的出站前一刻，
/// 此时改写已完成、尚未发出。
pub fn capture_request_pair(capture_id: &str, client_body: &[u8], upstream_body: &[u8]) {
    write_capture(capture_id, "_client.json", client_body);
    write_capture(capture_id, "_upstream.json", upstream_body);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 抓取编号必须形如 `<日期8位>_<时间6位>_<毫秒3位>_<序号3位>`，且同进程内不重复。
    #[test]
    fn capture_id_shape_and_uniqueness() {
        let first = new_capture_id();
        let second = new_capture_id();
        // 形态：日期_时间_毫秒_序号（下划线分隔，全数字段）。
        for id in [&first, &second] {
            let parts: Vec<&str> = id.split('_').collect();
            assert_eq!(parts.len(), 4, "编号应为四段：{id}");
            assert_eq!(parts[0].len(), 8, "日期段应为 8 位：{id}");
            assert_eq!(parts[1].len(), 6, "时间段应为 6 位：{id}");
            assert_eq!(parts[2].len(), 3, "毫秒段应为 3 位：{id}");
            assert_eq!(parts[3].len(), 3, "序号段应为 3 位：{id}");
            assert!(id.chars().all(|c| c.is_ascii_digit() || c == '_'));
        }
        assert_ne!(first, second, "同进程内编号不得重复");
    }

    /// 空编号是"未启用"的哨兵：两个写入函数都必须直接跳过（不建目录、不写文件）。
    #[test]
    fn empty_id_is_a_no_op() {
        // 不应 panic，也不应产生任何副作用（静默语义）。
        write_capture("", "_client.json", b"{}");
        append_capture("", "_upstream_resp.ndjson", b"data: x\n\n");
        capture_request_pair("", b"a", b"b");
    }

    /// 写入 + 追加 + 请求对：文件名按编号落盘且内容逐字一致。
    ///
    /// 用模块内覆盖重定向到独立临时目录：并发测试对 home 的覆盖是进程级
    /// 环境变量，多线程互相污染（写进 A 线程的临时 home、读时变成 B 的），
    /// 模块内覆盖与 home 彻底解耦，可并发安全地反复运行。
    #[test]
    fn write_append_pair_roundtrip() {
        let isolated = std::env::temp_dir().join(format!(
            "capture-roundtrip-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        set_test_root(Some(isolated.clone()));
        let id = new_capture_id();
        capture_request_pair(&id, b"{\"client\":1}", b"{\"upstream\":2}");
        append_capture(&id, "_upstream_resp.ndjson", b"event:a\n\n");
        append_capture(&id, "_upstream_resp.ndjson", b"event:b\n\n");

        let dir = capture_dir();
        let read = |suffix: &str| {
            let path = dir.join(format!("{id}{suffix}"));
            std::fs::read(&path).unwrap_or_else(|error| {
                panic!(
                    "读取 {path:?} 失败：{error}；目录存在：{:?}；目录内容：{:?}",
                    dir.exists(),
                    std::fs::read_dir(&dir)
                        .map(|entries| {
                            entries
                                .filter_map(|entry| entry.ok())
                                .map(|entry| entry.file_name().to_string_lossy().to_string())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                )
            })
        };
        assert_eq!(read("_client.json"), b"{\"client\":1}");
        assert_eq!(read("_upstream.json"), b"{\"upstream\":2}");
        assert_eq!(read("_upstream_resp.ndjson"), b"event:a\n\nevent:b\n\n");

        // 覆盖恢复 + 测试产物自清理（临时目录整个移除，不留垃圾）。
        set_test_root(None);
        let _ = std::fs::remove_dir_all(&isolated);
    }
}
