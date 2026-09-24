//! 账号选择策略（`current` / `pinned` / `max_credits`；P0-8）。

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use buddy_switch_core::modules::account;
use buddy_switch_core::modules::auth_file;
use buddy_switch_core::modules::config as core_config;
use buddy_switch_core::modules::credits;
use buddy_switch_core::modules::region::{self, Region, RegionMismatch};

use crate::error::GatewayError;

/// 账号选择策略（按 region 各自独立配置）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AccountStrategy {
    /// 默认：读该 region 认证文件指向的账号（跟随桌面端登录态）。
    Current,
    /// 固定账号：账号库按 id 选择。
    Pinned { account_id: String },
    /// 积分最多：在账号库中选剩余积分最高者。
    MaxCredits,
}

impl Default for AccountStrategy {
    fn default() -> Self {
        AccountStrategy::Current
    }
}

/// 以严格大于选择积分最高账号；并列时保留先出现者。
fn select_max_credit_candidate<I>(candidates: I) -> Option<Value>
where
    I: IntoIterator<Item = (f64, Value)>,
{
    let mut best: Option<(f64, Value)> = None;
    for (remaining, account) in candidates {
        if best
            .as_ref()
            .map(|(current, _)| remaining > *current)
            .unwrap_or(true)
        {
            best = Some((remaining, account));
        }
    }
    best.map(|(_, account)| account)
}

/// 账号选择器。
pub struct AccountSelector;

impl AccountSelector {
    /// 返回该 region 选中的账号（含 `access_token` / `domain` / `uid`）。
    ///
    /// 选中的账号会做**安全红线 F** 校验：`region_of(domain)` 必须等于目标
    /// region，否则返回 [`GatewayError::RegionMismatch`]（含修复指引）。
    pub async fn select(
        &self,
        region: Region,
        strategy: &AccountStrategy,
    ) -> Result<Value, GatewayError> {
        let account = match strategy {
            AccountStrategy::Current => {
                // 安全红线 F：先用 core 的校验读取（region 不符 → RegionMismatch，
                // 且不发起任何上游请求），再导入账号并二次防御校验。
                auth_file::read_auth_file_checked_for(region)
                    .map_err(GatewayError::RegionMismatch)?;
                let account = auth_file::import_from_auth_file_for(region)
                    .ok_or(GatewayError::NoCredential { region })?;
                ensure_region(&account, region)?;
                Ok(account)
            }
            AccountStrategy::Pinned { account_id } => {
                let account = account::find_account_for(region, account_id)
                    .ok_or(GatewayError::NoCredential { region })?;
                ensure_region(&account, region)?;
                Ok(account)
            }
            AccountStrategy::MaxCredits => {
                let accounts = account::load_accounts_for(region);
                if accounts.is_empty() {
                    return Err(GatewayError::NoCredential { region });
                }
                let mut candidates = Vec::with_capacity(accounts.len());
                for account in accounts {
                    let credits_value = credits::get_credit_expiry(&account).await;
                    let remaining = credits_value
                        .get("totalRemaining")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0);
                    candidates.push((remaining, account));
                }
                let account = select_max_credit_candidate(candidates.into_iter())
                    .expect("accounts is non-empty");
                ensure_region(&account, region)?;
                Ok(account)
            }
        }?;

        // 加密信封凭据不得送进网关：`build_chat_headers` 会把信封折成**空 `Bearer`**，
        // 上游只会回 401，而用户看到的是网关的 HTML 错误页。
        //
        // ★ 这条是「本机导入接受信封」（core `imported_account_from_root` 的
        //   `plain_text_or_envelope` 改造）的**必要配套**：改造之前 `Current` 策略会因
        //   导入失败而报 `NoCredential`（响亮、可行动），改造之后就会静默发出空 Bearer。
        //   三种策略都可能选出信封账号（`Pinned` / `MaxCredits` 走账号库，而库里在
        //   明文过期后也会被信封接管），故拦在 match 之后而不是各分支里。
        if account::is_envelope(&account, "access_token") {
            return Err(GatewayError::EncryptedCredential { region });
        }
        Ok(account)
    }
}

/// 构造某 region 的策略展示对象：`{ region, strategy, selected, error?, note? }`。
///
/// `selected` 只做**本地**解析（`current` 读认证文件 / `pinned` 查账号库），
/// 因此该接口恒定快速、无网络请求。`max_credits` 需要按实时积分对**全部**账号
/// 逐个发起请求择优，不适合在管理页轮询时预取，故返回 `selected: null` 并附
/// `note` 说明，由前端提示「每次请求时实时择优」。
pub async fn describe_strategy(region: Region, strategy: &AccountStrategy) -> Value {
    let mut described = serde_json::json!({
        "region": region,
        "strategy": serde_json::to_value(strategy).unwrap_or(Value::Null),
        "selected": Value::Null,
    });
    match strategy {
        AccountStrategy::MaxCredits => {
            described["note"] =
                serde_json::json!("max_credits 在每次请求时按实时积分择优，此处不预取（避免 N 次网络请求）");
        }
        _ => match AccountSelector.select(region, strategy).await {
            Ok(account) => {
                // account_meta 已剥离 token，可直接下发前端。
                described["selected"] = account::account_meta(&account);
            }
            Err(error) => {
                described["error"] = serde_json::json!(error.message());
            }
        },
    }
    described
}

/// 校验凭据 domain 所属 region 与目标 region 一致（安全红线 F）。
pub fn ensure_region(account: &Value, region: Region) -> Result<(), GatewayError> {
    let domain = account
        .get("domain")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let actual = region::region_of(&domain);
    if actual == region {
        return Ok(());
    }
    let spec = region::region_spec(region);
    Err(GatewayError::RegionMismatch(RegionMismatch {
        actual_domain: domain,
        expected_file: spec.auth_filename.to_string(),
        env_var: spec.auth_env.to_string(),
        actual_region: actual,
        expected_region: region,
    }))
}

/// 策略持久化文件（与 `gateway_config.json` 同目录）。
pub fn strategy_file() -> PathBuf {
    region::gateway_config_file().with_file_name("gateway_strategy.json")
}

/// 读取各 region 策略（缺失回落空表 → 各处默认 `current`）。
pub fn load_strategies() -> HashMap<Region, AccountStrategy> {
    let file = strategy_file();
    if let Ok(text) = std::fs::read_to_string(&file) {
        if let Ok(map) = serde_json::from_str::<HashMap<Region, AccountStrategy>>(&text) {
            return map;
        }
    }
    HashMap::new()
}

/// 保存各 region 策略。
pub fn save_strategies(map: &HashMap<Region, AccountStrategy>) -> Result<(), String> {
    let file = strategy_file();
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_json::to_string_pretty(map).map_err(|error| error.to_string())?;
    core_config::atomic_write(&file, &content).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn max_credits_selection_keeps_first_tie_and_uses_zero_for_missing_values() {
        let candidates = vec![
            (10.0, json!({"id": "first"})),
            (10.0, json!({"id": "tie"})),
            (0.0, json!({"id": "missing"})),
        ];
        let selected = select_max_credit_candidate(candidates.into_iter()).unwrap();
        assert_eq!(selected["id"], "first");

        let empty: Vec<(f64, Value)> = Vec::new();
        assert!(select_max_credit_candidate(empty.into_iter()).is_none());
    }

    #[test]
    fn strategy_serde_roundtrip() {
        assert_eq!(
            serde_json::to_value(AccountStrategy::Current).unwrap(),
            json!({"kind": "current"})
        );
        assert_eq!(
            serde_json::to_value(AccountStrategy::Pinned {
                account_id: "a1".into()
            })
            .unwrap(),
            json!({"kind": "pinned", "account_id": "a1"})
        );
        assert_eq!(
            serde_json::to_value(AccountStrategy::MaxCredits).unwrap(),
            json!({"kind": "max_credits"})
        );

        let parsed: AccountStrategy = serde_json::from_value(json!({"kind": "pinned", "account_id": "x"})).unwrap();
        assert!(matches!(parsed, AccountStrategy::Pinned { account_id } if account_id == "x"));
    }

    #[test]
    fn ensure_region_documents_legacy_missing_domain_behavior() {
        // CN defaults unknown/legacy records for backward compatibility;
        // Global remains fail-closed because an absent domain is not global evidence.
        let legacy = json!({"access_token": "t"});
        assert!(ensure_region(&legacy, Region::Cn).is_ok());
        let err = ensure_region(&legacy, Region::Global).unwrap_err();
        assert_eq!(err.status(), 401);
        assert_eq!(err.type_name(), "region_mismatch");
    }

    #[test]
    fn ensure_region_accepts_matching_and_rejects_mismatch() {
        let cn = json!({"domain": "www.codebuddy.cn", "access_token": "t"});
        assert!(ensure_region(&cn, Region::Cn).is_ok());

        let global = json!({"domain": "www.workbuddy.ai", "access_token": "t"});
        assert!(ensure_region(&global, Region::Global).is_ok());

        // 国际凭据出现在国内 region → 拒绝
        let err = ensure_region(&global, Region::Cn).unwrap_err();
        assert_eq!(err.status(), 401);
        assert_eq!(err.type_name(), "region_mismatch");
        let message = err.message();
        assert!(message.contains("workbuddy.ai"), "含实际 domain: {message}");
        // core 的 RegionMismatch::message() 由「实际 region 展示名 / 期望 region 展示名 /
        // 实际 domain / 环境变量名 / 期望 region 展示名」构成，不直接含认证文件名。
        assert!(
            message.contains(region::region_display(Region::Cn)),
            "含期望 region 展示名: {message}"
        );
        assert!(message.contains("WORKBUDDY_AUTH_FILE"), "含环境变量名: {message}");
    }
}
