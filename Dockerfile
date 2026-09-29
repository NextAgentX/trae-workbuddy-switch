# syntax=docker/dockerfile:1

# =============================================================================
# Buddy Switch —— 容器镜像（webui 服务 + API 网关）
# =============================================================================
#
# 这个镜像只跑 **服务端形态**（`buddy-switch serve`）：本地 webui 界面 + OpenAI /
# Anthropic 兼容的 API 网关。**不包含桌面 App**，因此依赖本机客户端的操作
# （切换本机客户端登录态、导入本机账号、重启 WorkBuddy / Trae 客户端）在容器里
# 无对象可用，也不会生效——那部分请继续用桌面 App。
#
# 容器里的正经用法：
#   · 账号库托管（OAuth 扫码 / 粘贴 JWT / 导入备份文件）
#   · 自动签到、积分与 Token 统计
#   · 把模型额度以 OpenAI / Anthropic 兼容接口对外提供（网关）
#
# --- 构建顺序（很重要）-----------------------------------------------------
# 前端产物是 **编译期** 经 `rust-embed` 嵌进服务端二进制的，所以必须先构建
# `dist/`，再编译 Rust。交换顺序会得到一个内嵌空目录的镜像。
#
# --- 体积 / 缓存策略 -------------------------------------------------------
# · 依赖单独一层：先只 COPY manifest（Cargo.toml / Cargo.lock）做一次
#   `cargo fetch`，源码变更时不会重下依赖。
# · 最终镜像只带二进制 + CA 证书，不带工具链。

# -----------------------------------------------------------------------------
# Stage 1 —— 构建前端（Node 20，与 CI 的 build.yml 保持一致）
# -----------------------------------------------------------------------------
FROM node:20-bookworm-slim AS frontend

WORKDIR /build

# 只拷依赖清单，先装依赖，利用 Docker 层缓存
COPY package.json package-lock.json ./
# `npm ci` 严格按 lockfile 安装；含 tauri CLI 等 devDependencies（vite build 需要）
RUN npm ci

# 拷入前端源码与构建脚本
COPY index.html vite.config.ts tsconfig.json tsconfig.node.json components.json ./
COPY public ./public
COPY src ./src
COPY scripts ./scripts

# `npm run build` 会依次执行 tsc + 三个 check 脚本 + vite build。
# 直接用它（而不是 `vite build`）以保持与 CI 同一套校验：类型错误、API 契约漂移、
# store selector 与 npm 平台检查都会在构建期拦住。
RUN npm run build

# -----------------------------------------------------------------------------
# Stage 2 —— 构建 Rust 服务端（把 Stage 1 的 dist/ 嵌进去）
# -----------------------------------------------------------------------------
FROM rust:1-bookworm AS backend

WORKDIR /build

# 先只拷 manifest，预拉依赖（源码变更不触发重新下载）
COPY Cargo.toml Cargo.lock ./
COPY crates/buddy-switch-core/Cargo.toml    crates/buddy-switch-core/Cargo.toml
COPY crates/buddy-switch-gateway/Cargo.toml crates/buddy-switch-gateway/Cargo.toml
COPY crates/buddy-switch-server/Cargo.toml  crates/buddy-switch-server/Cargo.toml
COPY src-tauri/Cargo.toml                   src-tauri/Cargo.toml

# workspace 成员的 src/ 目录必须存在，否则 `cargo fetch` 无法解析 workspace。
# 这里用空文件占位，只为过 `cargo fetch` 这一层；真正源码在后面覆盖。
RUN set -eux; \
    for pkg in buddy-switch-core buddy-switch-gateway buddy-switch-server; do \
        mkdir -p "crates/$pkg/src"; \
        : > "crates/$pkg/src/lib.rs"; \
    done; \
    mkdir -p crates/buddy-switch-server/src; \
    : > crates/buddy-switch-server/src/main.rs; \
    mkdir -p src-tauri/src; \
    : > src-tauri/src/lib.rs; \
    : > src-tauri/src/main.rs

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo fetch --locked

# 拷入真正的后端源码与前端产物
COPY crates ./crates
COPY src-tauri ./src-tauri
COPY --from=frontend /build/dist ./dist

# 只编服务端这一个 package（不编 Tauri 桌面壳——它需要系统 WebKit 依赖）。
#
# ⚠️ `dist/` 必须先就位：`buddy-switch-server` 通过 `rust-embed` 在**编译期**读取它。
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release --locked -p buddy-switch-server && \
    cp /build/target/release/buddy-switch /build/buddy-switch

# -----------------------------------------------------------------------------
# Stage 3 —— 运行时
# -----------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# ca-certificates：网关要向上游发起 HTTPS 请求（OAuth、积分、模型接口）
# tzdata：签到排程按本地时区触发；容器默认 UTC，这里让 TZ 环境变量生效
# curl：仅用于 HEALTHCHECK 探测 webui 是否真的在响应
RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends ca-certificates tzdata curl; \
    rm -rf /var/lib/apt/lists/*

# 非 root 运行。固定 uid/gid 便于宿主侧授权数据目录。
RUN set -eux; \
    groupadd --gid 10001 buddy; \
    useradd --uid 10001 --gid 10001 --create-home --shell /usr/sbin/nologin buddy

COPY --from=backend /build/buddy-switch /usr/local/bin/buddy-switch

# 数据目录：账号库、配置、网关状态、日志都在这里（由 BUDDY_SWITCH_HOME 指向）。
# 部署时挂卷到 /data 即可持久化。
ENV BUDDY_SWITCH_HOME=/data \
    BUDDY_SWITCH_HOST=0.0.0.0 \
    BUDDY_SWITCH_PORT=57890 \
    TZ=Asia/Shanghai

RUN mkdir -p /data && chown -R buddy:buddy /data

USER buddy
WORKDIR /home/buddy

# 57890 = webui；57891 = WorkBuddy API 网关；7864 = Trae API 网关
EXPOSE 57890 57891 7864

# 入口脚本：无参数时按 BUDDY_SWITCH_PORT 起 webui 服务；
# 传了参数（如 `status` / `version` / `serve --port 9`）则原样透传，
# 保持 `docker run <image> status` 这类用法可用。
COPY --chown=buddy:buddy docker/entrypoint.sh /usr/local/bin/entrypoint.sh

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]

# 健康检查：探测 webui 根路径真的能返回 HTTP 响应（而不是只看进程在不在）。
# 走 BUDDY_SWITCH_PORT，容器内改端口时自动跟随。
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl -fsS -o /dev/null "http://127.0.0.1:${BUDDY_SWITCH_PORT}/" || exit 1
