//! Trae 模块的**测试专用**环境隔离与 fixture 构造。
//!
//! 本模块只在 `cfg(test)` 下编译，生产代码零引用。
//!
//! ## 为什么必须集中在一处（而不是各测试模块各写一份）
//!
//! 「改进程级环境变量」是本仓库最容易写出**假绿**与**随机红**的地方：
//! [`crate::modules::config::env_lock`] 只保护**同样拿了锁**的用例，
//! 而 `BUDDY_SWITCH_HOME` 与 `APPDATA` 是两个独立变量、却共用同一份「当前环境」。
//! 常见的三种错法都会让失败在**别的**用例里冒出来，极难定位：
//!
//! 1. **少拿一次锁** —— 别的用例正在读环境，读到一半被改掉；
//! 2. **还原与删目录的顺序反了** —— 变量继续指向一个已不存在的目录，
//!    后续用例吃一次「已忽略」警告并**静默回落真实 home**；
//! 3. **只隔离其中一个变量** —— 隔离了 home 却没隔离 `APPDATA`，
//!    于是用例读到真机的 Trae 数据目录（真机上恰好有数据时「碰巧」通过）。
//!
//! 因此只保留这一份实现：**新用例一律用 [`TempEnv`]，不要再手写 `set_var` 序列。**
//!
//! ## 与 [`crate::modules::trae::icube::test_support`] 的分工
//!
//! 那边造的是**文件内容**（tc 信封、合成密钥对）；
//! 这边管的是**环境**（把进程指向临时目录）。两者组合使用。

use std::path::PathBuf;
use std::sync::MutexGuard;

use crate::modules::config::BUDDY_SWITCH_HOME_ENV;

/// 隔离后的临时环境：`BUDDY_SWITCH_HOME` 与 `APPDATA` 都指向本实例私有目录。
///
/// drop 时**先还原两个变量、再删临时目录**（顺序不可反，理由见模块文档）。
pub(crate) struct TempEnv {
    root: PathBuf,
    previous_home: Option<std::ffi::OsString>,
    previous_appdata: Option<std::ffi::OsString>,
    /// 只为持有 `env_lock`；不直接读取，故带下划线前缀。
    _lock: MutexGuard<'static, ()>,
}

impl TempEnv {
    /// 隔离 home 与 `APPDATA`，并铺一份合成**设备凭证** fixture。
    ///
    /// 覆盖两条产品线的**全部候选目录**（各给不同 `deviceId`），
    /// 供需要 `iCubeAuthInfo://icube-dc:*` 的用例使用（如 OAuth 发起登录）。
    pub(crate) fn with_device_fixture() -> Self {
        Self::new(true)
    }

    /// 隔离 home 与 `APPDATA`，**不铺任何 fixture**（由调用方自行铺）。
    ///
    /// 调用方拿 [`TempEnv::appdata`] 当「客户端 userData 的父目录」用：
    /// 在里面建 `<产品线目录>/User/globalStorage/storage.json` 即可。
    ///
    /// 只在 Windows 上提供：`platform::data_dir_base()` 只在 Windows 分支读环境变量，
    /// 本仓库所有依赖「客户端数据目录」的用例也都是 `#[cfg(windows)]`（既有约定）。
    #[cfg(windows)]
    pub(crate) fn empty() -> Self {
        Self::new(false)
    }

    /// 本次用例私有的「客户端 userData 父目录」（= 临时 `APPDATA`）。
    #[cfg(windows)]
    pub(crate) fn appdata(&self) -> PathBuf {
        self.root.join("appdata")
    }

    fn new(with_device_fixture: bool) -> Self {
        // ★ 只取**一把** `env_lock()`：`Mutex` 非可重入，若这里再用
        //   `HomeOverrideGuard`（它内部也取同一把锁）会自锁。
        //   下面直接改两个变量，效果等价且只持一把锁。
        let lock = crate::modules::config::env_lock();

        let root = std::env::temp_dir().join(format!(
            "buddy-switch-trae-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let home = root.join("home");
        let appdata = root.join("appdata");
        std::fs::create_dir_all(&home).expect("临时 home 应能创建");
        std::fs::create_dir_all(&appdata).expect("临时 appdata 应能创建");
        if with_device_fixture {
            crate::modules::trae::icube::test_support::write_synthetic_user_data(&appdata);
        }

        let previous_home = std::env::var_os(BUDDY_SWITCH_HOME_ENV);
        let previous_appdata = std::env::var_os("APPDATA");
        // `BUDDY_SWITCH_HOME` 是唯一被识别的 home 覆盖变量。
        std::env::set_var(BUDDY_SWITCH_HOME_ENV, &home);
        std::env::set_var("APPDATA", &appdata);

        Self {
            root,
            previous_home,
            previous_appdata,
            _lock: lock,
        }
    }
}

impl Drop for TempEnv {
    fn drop(&mut self) {
        match self.previous_home.take() {
            Some(value) => std::env::set_var(BUDDY_SWITCH_HOME_ENV, value),
            None => std::env::remove_var(BUDDY_SWITCH_HOME_ENV),
        }
        match self.previous_appdata.take() {
            Some(value) => std::env::set_var("APPDATA", value),
            None => std::env::remove_var("APPDATA"),
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
