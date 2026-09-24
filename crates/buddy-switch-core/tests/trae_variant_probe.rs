//! 真机探针：验证变体探测在**本机实际目录**上的行为。
//!
//! 这是唯一能证明「探测到的是哪条产品线」在真实环境可用的手段 ——
//! 单元测试都在临时目录里造数据，无法回答「本机现在被判定成什么」。
//!
//! 只读：不写任何文件、不改客户端状态。
//! 运行：`cargo test -p buddy-switch-core --test trae_variant_probe -- --ignored --nocapture`

use buddy_switch_core::modules::trae::platform;
use buddy_switch_core::modules::trae::variant::{self, TraeVariant};

#[test]
#[ignore = "探针：读取本机真实环境，默认不跑；用 --ignored 显式触发"]
fn probe_real_machine_variant_detection() {
    println!("\n=== 本机 Trae 变体探测 ===");

    let env = platform::env_status();
    println!("env_status = {}", serde_json::to_string_pretty(&env).unwrap());

    let variant = env
        .get("variant")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let label = env
        .get("variantLabel")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    println!("探测到的变体 = {variant:?} / 展示名 = {label:?}");

    // 如果探测出了变体，它必须能被反向解析回同一个变体（表自洽）。
    if let Some(id) = &variant {
        let parsed = TraeVariant::parse(id).expect("variant 字段必须能被 parse 回变体");
        assert_eq!(parsed.display_name(), label.as_deref().unwrap_or(""));
    }

    println!("\n=== 各变体候选目录的存在性（本机实测） ===");
    // `APPDATA` 在 Git Bash 下**可能是空字符串**（不是未设置），此时
    // `unwrap_or_default()` 会得到空路径、候选全部拼成相对路径而「不存在」。
    // 必须回退到 `dirs::config_dir()`，否则探针会给出误导性的结论。
    let base = std::env::var("APPDATA")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(std::path::PathBuf::from)
        .or_else(dirs::config_dir)
        .expect("必须能定位 userData 根目录");
    println!("userData 根目录 = {}", base.display());
    for v in TraeVariant::all() {
        println!("--- {v:?} ({}) ---", v.display_name());
        for dir in variant::variant_spec(v).data_dir_candidates(&base) {
            println!(
                "  {} exists={}",
                dir.display(),
                dir.is_dir()
            );
        }
    }

    println!("\n=== 各变体的**安装探测**与**目录选择**（OAuth 前置条件） ===");
    // 这一节回答的是「OAuth 网页登录能不能发起」：
    //   ① `detect_install_for` 找不到客户端 ⇒ `trae_launch_client` 会响亮失败（给不出可执行文件）；
    //   ② `select_data_dir_for` 为 `None` ⇒ 没有 userData ⇒ `device_identity_for` 必然
    //      `DataDirMissing`，**用户点多少次「重新发起登录」都没用**（客户端首次启动才写凭证）。
    // 两个值都来自与生产代码**同一个函数**，因此这里的输出就是界面会看到的东西。
    for v in TraeVariant::all() {
        let probe = platform::detect_install_for(v);
        println!(
            "--- {v:?} ({}) --- installed={} version={:?} exe={:?}",
            v.display_name(),
            probe.installed,
            probe.version,
            probe.exe.as_ref().map(|p| p.display().to_string()),
        );
        // 「写侧」与「读/展示侧」两个选择器都给出来：它们语义不同、**允许不同值**，
        // 探针把两者并排打印正是为了让人一眼看出「报错说的是哪一个」。
        println!(
            "    写侧 detect_data_dir_for = {:?}",
            platform::detect_data_dir_for(v).map(|p| p.display().to_string())
        );
        println!(
            "    读侧 select_data_dir_for = {:?}（None ⇒ OAuth 必失败）",
            platform::select_data_dir_for(v).map(|p| p.display().to_string())
        );
    }

    println!("\n=== 各变体端点表 ===");
    for v in TraeVariant::all() {
        let ep = variant::variant_spec(v);
        println!(
            "{:?}: cn.account={} cn.agent={} global.account={:?}",
            v,
            ep.cn_endpoints.account_base,
            ep.cn_endpoints.agent_host,
            ep.global_endpoints.as_ref().map(|g| g.account_base),
        );
    }

    println!("\n=== 各变体的凭据可取性（只读；不打印 token 本身） ===");
    for v in TraeVariant::all() {
        println!("--- {v:?} ({}) ---", v.display_name());
        for dir in variant::variant_spec(v).data_dir_candidates(&base) {
            if !dir.is_dir() {
                continue;
            }
            println!("  数据目录 = {}", dir.display());
            // 是否装了会记录 `Authorization` 头的扩展 —— 这是明文凭据的**唯一**来源，
            // 缺了它无论怎么重试导入都拿不到（实测 `TRAE SOLO CN` 就是这种情况）。
            let ext = dir
                .join("logs")
                .is_dir()
                .then(|| find_extension_dirs(&dir))
                .unwrap_or_default();
            println!("  日志里的扩展目录 = {ext:?}（凭据来源扩展 trae.ai-code-completion 存在={}）",
                ext.iter().any(|e| e == "trae.ai-code-completion"));
        }
    }
}

/// 列出该 userData 下 `logs/*/window*/exthost/*` 的一级目录名（去重、排序）。
///
/// 只读目录结构，**不打开日志正文** —— 探针必须在「不回显凭据」的前提下也能
/// 回答「为什么提不到凭据」。
fn find_extension_dirs(data_dir: &std::path::Path) -> Vec<String> {
    let mut out = std::collections::BTreeSet::new();
    let Ok(sessions) = std::fs::read_dir(data_dir.join("logs")) else {
        return Vec::new();
    };
    for session in sessions.flatten() {
        let Ok(windows) = std::fs::read_dir(session.path()) else {
            continue;
        };
        for window in windows.flatten() {
            let Ok(exts) = std::fs::read_dir(window.path().join("exthost")) else {
                continue;
            };
            for ext in exts.flatten() {
                if let Some(name) = ext.file_name().to_str() {
                    out.insert(name.to_string());
                }
            }
        }
    }
    out.into_iter().collect()
}
