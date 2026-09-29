# 容器部署（Docker / Docker Compose）

把 Buddy Switch 以 **服务端形态** 跑在容器里：本地 webui 界面 + OpenAI / Anthropic 兼容的 API 网关。

> ⚠️ 容器里 **不包含桌面 App**。依赖本机客户端的操作（切换本机客户端登录态、导入本机账号、重启 WorkBuddy / Trae 客户端）在容器中无对象可用，也不会生效——那部分请继续用桌面 App。容器里的正经用途是：账号库托管、自动签到、积分与 Token 统计、以及把额度以兼容接口对外提供。

---

## 1. 快速开始

```bash
docker compose up -d --build
docker compose logs -f
```

日志里会打印：

```
webui: http://0.0.0.0:57890
API 网关: http://0.0.0.0:57891      # 若已在配置里开启网关
Trae API 网关: http://0.0.0.0:7864  # 若已在配置里开启 Trae 网关
```

浏览器打开 <http://127.0.0.1:57890> 即可。

停服务（数据保留在卷里）：

```bash
docker compose down          # 保留数据卷
docker compose down -v       # ⚠️ 连数据卷一起删，账号库会丢
```

---

## 2. 数据持久化

所有持久化数据都在 `BUDDY_SWITCH_HOME` 指向的目录（容器内 `/data`），compose 里已挂到命名卷 `buddy-switch-data`：

```
buddy-switch-data  →  /data
    ├── accounts.json          账号库（token / JWT）
    ├── backups/               切换前的自动备份
    ├── gateway_state.json     网关配置（监听地址、端口、Key 归属）
    ├── gateway_keys.json      网关 API Key
    └── logs/                  操作日志
```

**备份**：直接打包这个卷即可，账号 token 与网关 API Key 都在里面。

```bash
docker run --rm -v buddy-switch-data:/data -v "$PWD:/backup" alpine \
  tar czf /backup/buddy-switch-backup.tar.gz -C /data .
```

**改用宿主目录**（方便直接看文件）：把 compose 里的卷换成绑定挂载。

```yaml
volumes:
  - ./data:/data      # 注意执行前先 mkdir -p ./data && chown 10001:10001 ./data
```

容器内以 uid/gid `10001` 运行，宿主目录需要属于该 uid（或放宽容许）。

---

## 3. 开启 API 网关

网关能对外提供额度，但**默认监听回环地址**，容器里必须显式放开，否则端口映射进来也连不通。

打开 webui → 侧栏「API 服务」→ 把监听地址从 `127.0.0.1` 改为 `0.0.0.0`，并确认允许非回环监听。保存后网关会用新地址重启。

> 程序内置了一道安全门：监听地址不是回环时必须同时打开 `allow_non_loopback`，否则会拒绝启动并给出提示。这是防止误把额度暴露到公网的护栏。

开启后即可用兼容接口访问：

```bash
# OpenAI 兼容
curl http://127.0.0.1:57891/v1/models \
  -H "Authorization: Bearer <你在 webui 里创建的 Key>"

curl http://127.0.0.1:57891/v1/chat/completions \
  -H "Authorization: Bearer <Key>" \
  -H "Content-Type: application/json" \
  -d '{"model":"<模型名>","messages":[{"role":"user","content":"hi"}],"stream":true}'

# Anthropic 兼容
curl http://127.0.0.1:57891/v1/messages \
  -H "x-api-key: <Key>" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{"model":"<模型名>","max_tokens":256,"messages":[{"role":"user","content":"hi"}]}'
```

Trae 分区用 `7864` 端口，用法一致，Key 与账号池按分区隔离。

---

## 4. 端口与监听

| 端口 | 用途 | 鉴权 |
| --- | --- | --- |
| `57890` | webui 界面 | **无鉴权**——谁能连上谁就能操作账号库 |
| `57891` | WorkBuddy API 网关 | 网关 API Key |
| `7864` | Trae API 网关 | 网关 API Key |

compose 默认把三者都绑在宿主 `127.0.0.1`，只有本机能访问。要跨机使用，推荐的做法是**保持回环绑定 + 前置反向代理**（Caddy / Nginx）加 TLS 与访问控制，而不是直接把 `57890` 暴露到公网。

改容器内端口：改 `BUDDY_SWITCH_PORT` 环境变量，并同步 `ports` 段与健康检查。入口脚本与健康检查都会跟随该变量。

---

## 5. 环境变量

| 变量 | 默认 | 说明 |
| --- | --- | --- |
| `BUDDY_SWITCH_HOME` | `/data` | 数据目录。**改它记得同步卷挂载点。** |
| `BUDDY_SWITCH_HOST` | `0.0.0.0` | webui 监听地址。默认 `0.0.0.0` 以便端口映射生效；不设时程序回落到 `127.0.0.1`。 |
| `BUDDY_SWITCH_PORT` | `57890` | webui 端口。 |
| `TZ` | `Asia/Shanghai` | 影响签到等排程任务的触发时刻。 |
| `HTTPS_PROXY` / `HTTP_PROXY` | 无 | 需要经代理访问上游时设置。 |

> `BUDDY_SWITCH_HOST` 是本次新增的：程序默认仍只监听 `127.0.0.1`（保证本地直装用户不被意外暴露），容器 / 反代部署时由部署者显式设为 `0.0.0.0`。

---

## 6. 不用 compose 的纯 docker 用法

```bash
# 构建
docker build -t buddy-switch:local .

# 起服务
docker run -d --name buddy-switch \
  -p 127.0.0.1:57890:57890 \
  -p 127.0.0.1:57891:57891 \
  -p 127.0.0.1:7864:7864 \
  -e BUDDY_SWITCH_HOME=/data \
  -e BUDDY_SWITCH_HOST=0.0.0.0 \
  -v buddy-switch-data:/data \
  --restart unless-stopped \
  buddy-switch:local

# 透传子命令（入口脚本支持）
docker run --rm -v buddy-switch-data:/data buddy-switch:local status
docker run --rm buddy-switch:local version
```

---

## 7. 常见问题

**容器起来了，宿主浏览器打不开。**
检查 `BUDDY_SWITCH_HOST` 是否为 `0.0.0.0`——留空会回落回环，端口映射就连不进来。

**网关端口映射了，但连不上。**
网关默认只监听回环，需要在 webui「API 服务」里改成 `0.0.0.0` 并允许非回环监听（见第 3 节）。

**签到时间不对。**
容器的 `TZ` 默认是 `Asia/Shanghai`，改 compose 里的 `TZ` 后需要重启容器。

**宿主机上已经有服务占用了 57890。**
改 compose 的 `ports` 左值（宿主侧端口）即可，容器内不用动。

**为什么镜像里没有桌面 App？**
桌面 App 是 Tauri 打包，需要系统 WebKit 与图形环境，且它的核心能力（操作本机客户端）在容器里没有意义。镜像只编 `buddy-switch-server`。

**数据卷能跨版本升级吗？**
可以。账号库与网关配置是稳定的 JSON 文件，`down` 后重新 `build` + `up` 即可，卷不会被动。
