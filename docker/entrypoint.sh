#!/bin/sh
# Buddy Switch 容器入口。
#
# 语义：
#   · 不带参数            → 按 $BUDDY_SWITCH_PORT 启动 webui 服务（容器默认行为）
#   · 带参数              → 原样透传给 `buddy-switch`
#                          例：`docker run --rm <image> status`
#                              `docker run --rm <image> version`
#                              `docker run ... <image> serve --port 9000 --no-open`
#
# 为什么要有这层：直接把 ENTRYPOINT 写成 `sh -c "... --port $PORT"` 会把子命令
# 一起吞掉；写成 CMD 又无法让端口跟随环境变量。入口脚本两头都顾上。
set -eu

if [ "$#" -gt 0 ]; then
    exec buddy-switch "$@"
fi

: "${BUDDY_SWITCH_PORT:=57890}"
# --no-open：容器内没有浏览器，不加会让程序尝试拉起 xdg-open 并报错
exec buddy-switch serve --port "$BUDDY_SWITCH_PORT" --no-open
