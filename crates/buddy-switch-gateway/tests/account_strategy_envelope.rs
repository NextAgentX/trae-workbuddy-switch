//! 网关的**加密信封凭据**护栏：选中的账号只有信封凭据时必须**响亮失败**，
//! 不得静默发出空 `Bearer`。
//!
//! # 为什么必须有它（「修好一个缺陷顺手引入另一个」的典型形态）
//!
//! core 的「本机导入接受信封」改造（`imported_account_from_root` 改用
//! `plain_text_or_envelope`）让 `AccountStrategy::Current` **第一次**能返回一个
//! `access_token` 是信封对象的账号。改造之前那条路会因导入失败报 `NoCredential`
//! （响亮、可行动）；改造之后若没有这道护栏，`build_chat_headers` 会把信封折成
//! **空 `Bearer`**，上游回 401，而用户看到的是网关的整页 HTML 错误页。
//!
//! ⇒ 这类「接线漏了」的缺陷构建全绿、类型全对，只有**端到端断言**能挡住。
//!
//! # 环境隔离
//!
//! `AccountSelector::select` 会读 `~/.buddy-switch/` 账号库与认证文件，因此本测试把
//! `BUDDY_SWITCH_HOME` 指向一个**已存在**的临时目录（该变量要求目录存在，否则会被
//! 忽略并回落真实 home）。集成测试是独立进程，进程级 env 不会干扰 lib 单测；
//! 全部用例写在**同一个** `#[tokio::test]` 里，避免同进程内的 env 竞态。

use serde_json::json;

use buddy_switch_core::modules::auth_file;
use buddy_switch_core::modules::region::Region;
use buddy_switch_gateway::{AccountSelector, AccountStrategy};

#[tokio::test]
async fn envelope_credentials_are_refused_instead_of_sent_as_empty_bearer() {
    let home = std::env::temp_dir().join(format!("wb-gw-envelope-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("创建隔离目录");
    std::env::set_var("BUDDY_SWITCH_HOME", &home);

    let envelope = json!({"$wbEncrypted": 1, "envelope": "blob"});

    // ── Pinned：账号库里是信封凭据 ⇒ 必须报 EncryptedCredential ──
    let store = home.join(".buddy-switch");
    std::fs::create_dir_all(&store).expect("创建账号库目录");
    std::fs::write(
        store.join("accounts.json"),
        serde_json::to_string(&json!([
            {
                "id": "envelope-only",
                "uid": "uid-env",
                "domain": "www.codebuddy.cn",
                "access_token": envelope,
            },
            {
                "id": "plain",
                "uid": "uid-plain",
                "domain": "www.codebuddy.cn",
                "access_token": "PLAIN-AT",
            }
        ]))
        .expect("序列化账号库"),
    )
    .expect("写账号库");

    let error = AccountSelector
        .select(
            Region::Cn,
            &AccountStrategy::Pinned {
                account_id: "envelope-only".to_string(),
            },
        )
        .await
        .expect_err("信封凭据不得被选中送出");
    assert_eq!(error.type_name(), "encrypted_credential");
    assert_eq!(error.status(), 401);
    assert!(
        error.message().contains("加密信封"),
        "文案应可读且可行动：{}",
        error.message()
    );
    // ★ 与 `no_credential` 刻意分开：这条的用户其实**已经登录了**。
    assert_ne!(
        error.type_name(),
        "no_credential",
        "不得与「未找到已登录账号」混为一谈"
    );

    // 阳性对照：明文凭据照常选中（护栏不得误伤）。
    let selected = AccountSelector
        .select(
            Region::Cn,
            &AccountStrategy::Pinned {
                account_id: "plain".to_string(),
            },
        )
        .await
        .expect("明文凭据必须可用");
    assert_eq!(selected["id"], "plain");

    // ── Current：认证文件里是信封凭据 ⇒ 同样必须拦住 ──
    // 用 `auth_file_path_for` 算路径，与生产代码**同源**（不手拼平台相关路径）。
    let auth_path = auth_file::auth_file_path_for(Region::Cn);
    std::fs::create_dir_all(auth_path.parent().expect("auth parent")).expect("创建认证目录");
    std::fs::write(
        &auth_path,
        serde_json::to_string(&json!({
            "account": {"uid": "uid-current", "nickname": envelope},
            "auth": {"accessToken": envelope, "domain": "www.codebuddy.cn"},
        }))
        .expect("序列化认证文件"),
    )
    .expect("写认证文件");

    let error = AccountSelector
        .select(Region::Cn, &AccountStrategy::Current)
        .await
        .expect_err("Current 策略同样不得把信封凭据送出去");
    assert_eq!(error.type_name(), "encrypted_credential");

    let _ = std::fs::remove_dir_all(&home);
}
