# Buddy Switch

**WorkBuddy / TraeWork 多账号管理工具**（webui 形态）：启动本地服务后用浏览器操作，能力与桌面 App 一致。

> 桌面 App（Tauri）请从 [GitHub Releases](https://github.com/NextAgentX/trae-workbuddy-switch/releases/latest) 下载安装包。
> 在线只读演示：<https://nextagentx.github.io/trae-workbuddy-switch/>

> **本包（`@nextagentx/buddy-switch`）尚未发布到 npm**：下面的安装命令现在还不可用，需要 webui 形态请先按仓库 README 从源码构建。
> 发布后再以本页命令为准。

## 安装与运行

```bash
npm i -g @nextagentx/buddy-switch

buddy-switch              # 启动本地服务 + 自动打开浏览器
buddy-switch serve        # 只起服务，不开浏览器（--port 指定端口）
buddy-switch status       # 终端查看当前账号
buddy-switch version      # 版本号
```

默认监听 `127.0.0.1:57890`。二进制以「平台分包」形式发布（`@nextagentx/buddy-switch-<platform>-<arch>`），
主包把它声明为 `optionalDependencies`，安装时 npm 自动装好，`postinstall` 只负责把二进制复制到 `bin/`——
因此**不依赖 GitHub，国内镜像（npmmirror）也能稳定安装**。

## 两个产品分区

侧栏顶部切换产品分区，两个分区**彼此独立**（独立账号库、独立客户端、独立配置）：

| 分区 | 管理的客户端 | 区域 |
| --- | --- | --- |
| **WorkBuddy** | WorkBuddy 桌面客户端、CodeBuddy CLI、CodeBuddy CN IDE | 国内版（WorkBuddy）/ 国际版（WorkBuddy AI） |
| **TraeWork** | Trae Work（`TRAE SOLO CN` / `TRAE SOLO`）、Trae IDE（`Trae CN` / `Trae`） | 国内版 / 国际版 |

两个分区下的页面结构逐条同构：账号管理、Token 统计、积分统计、API 服务、设置。

## 功能

| 模块 | 说明 |
| --- | --- |
| 账号管理 | OAuth 扫码 / 网页登录、从本机导入、从备份文件导入、手动添加 token、删除账号、导出账号备份 |
| 账号切换 | WorkBuddy：备份认证文件 → 关闭客户端 → 写入目标账号 → 重启；Trae：写入客户端登录态并重启，切换前自动保存快照 |
| 会话复制 | 将当前账号勾选的会话以新 id 复制给目标账号（仅 WorkBuddy） |
| 自动签到 | WorkBuddy 六类定时任务（签到 / 猫猫旅行 / 活跃地图 / token 保活 / 开学季 / 夜猫子）各自可开关与配置小时；Trae 支持一键签到、跳过已签到与自动续期 |
| 积分到期 | 查询各账号积分资源与到期时间，7 天内到期高亮并按紧迫程度排序 |
| 积分统计 | 官方请求用量、每日趋势、模型分布、账号消耗与请求明细；官方数据不可用时明确回退本地余额快照 |
| Token 统计 | WorkBuddy / CodeBuddy CLI / CodeBuddy IDE 与 Trae 分别统计输入、输出、缓存读写与调用次数 |
| Token 保活 | 惰性刷新 + 每日保活，避免 refresh token 过期（仅 WorkBuddy） |
| CodeBuddy CLI | macOS/Linux 走 `apiKeyHelper`，Windows 走 `settings.json` 的 `env.CODEBUDDY_AUTH_TOKEN`；可配置自动轮换 |
| API 网关 | 把模型额度以 OpenAI / Anthropic 兼容接口暴露给本机其它工具；多 Key、按区域隔离、请求日志 |
| 自动更新 | 检查 GitHub Releases 新版本（桌面 App 支持签名校验整包更新） |

## 数据与隐私

- 所有数据保存在本机 `~/.buddy-switch/`，不上传到任何第三方服务
- 切换账号前会先备份原认证文件；Trae 切换登录态前会先保存快照
- 网关 API Key 只保存前缀与哈希，明文仅在创建时返回一次

## 参与贡献

**欢迎 PR 与 Issue** —— 报 Bug、补文档、适配新版客户端、改进界面都可以。提交前的检查项与几条硬性要求（不要提交本地数据与来源不明的代码、贡献按同一许可分发）见[贡献指南](https://github.com/NextAgentX/trae-workbuddy-switch/blob/main/CONTRIBUTING.md)。

## 支持这个项目

如果 Buddy Switch 帮到了你，可以请作者喝杯饮料 ☕

<table>
  <tbody>
    <tr>
      <td align="center"><img src="https://raw.githubusercontent.com/NextAgentX/trae-workbuddy-switch/main/docs/images/donate-wechat.png" alt="微信支付收款码" width="240" /><br />微信支付</td>
      <td align="center"><img src="https://raw.githubusercontent.com/NextAgentX/trae-workbuddy-switch/main/docs/images/donate-alipay.jpg" alt="支付宝收款码" width="240" /><br />支付宝</td>
    </tr>
  </tbody>
</table>

## 许可

[PolyForm Noncommercial License 1.0.0](https://github.com/NextAgentX/trae-workbuddy-switch/blob/main/LICENSE) —— **个人非商业使用许可，商业使用不被授权**；白话说明与常见问题见[许可说明](https://github.com/NextAgentX/trae-workbuddy-switch/blob/main/docs/LICENSING.md)；如需商业授权，请通过主仓库 Issues 联系作者。
