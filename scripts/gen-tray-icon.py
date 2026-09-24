#!/usr/bin/env python3
"""生成托盘图标素材（macOS 菜单栏 template / Windows·Linux 彩色）。

背景：两个平台对「颜色」的处理完全不同，这决定了本脚本产出**两份**素材。
  - macOS 菜单栏是 **template image**：系统只取 **alpha 通道**当形状，
    颜色由菜单栏前景色着色（浅色/深色模式自动适配）⇒ **RGB 被完全忽略**。
  - Windows / Linux 通知区**没有** template 语义（`icon_as_template` 是 no-op），
    RGB **原样生效** ⇒ 深色任务栏下黑色剪影几乎看不见，浅色任务栏下白色剪影
    同样几乎看不见。

结论：按平台分流素材——
  - `tray-icon-template.{png,rgba}`：36x36，RGB 一律填白色（`INK`）。
    macOS 专用，RGB 被忽略故无副作用。**不要**把它给 Windows/Linux 用。
  - `tray-icon-color.{png,rgba}`：32x32，RGB 取**源图真实颜色**、alpha 仍为同一掩码。
    Windows / Linux 专用，两种任务栏底色下都能看清。
alpha 通道始终才是形状的唯一来源，两份素材共用同一个 `build_mask()`。

做法：
  1. 去掉青绿底色（判据：R 明显低于 G 与 B），得到猫头整体轮廓；
  2. 把「被暗部完全包围的白色区域」挖成镂空——它们是猫的眼睛（白脸直接贴着青色
     背景，故不会被误判）；
  3. 把掩码缩放到目标尺寸，再按变体取色：
       - template：填充 `INK`；
       - color：按同一掩码从源图**保色下采样**（RGB 取源图、alpha 取掩码），
         透明处 RGB 置 0。
  4. 每个变体各写一份 `.rgba` 原始字节（`tray.rs` 用 `include_bytes!` 直接引用）
     与同名 `.png`（仅供预览/替换）。

⚠️ `tray.rs` 里有 `const ICON: &[u8; 36 * 36 * 4]` 与
`const COLOR_ICON: &[u8; 32 * 32 * 4]` 的**编译期长度断言**：
尺寸一旦不是 36 / 32，Rust 侧会直接编译失败，两者必须同步修改。

⚠️ **不要手工改产出的 PNG**。运行时真正生效的是 `.rgba`
（`tray.rs` 以 `include_bytes!` 引用），PNG 只是预览。手改 PNG 既不影响运行，
也会在下次跑本脚本时被静默覆盖，使两个产物不一致——要改就改这里的 `INK`
（template 填充色）或源图 `pic/logo.png`（color 的颜色来源）。

用法：python scripts/gen-tray-icon.py [源图路径]（默认 pic/logo.png）
"""

import sys
from collections import deque
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
ICONS = ROOT / "src-tauri" / "icons"
SIZE = 36          # macOS template 尺寸，必须与 tray.rs 的 36*36*4 断言一致
COLOR_SIZE = 32    # Windows/Linux 彩色尺寸，必须与 tray.rs 的 32*32*4 断言一致
WORK = 256         # 识别形状时的工作分辨率（最终仅 36/32px，无需全分辨率）
TEAL_DELTA = 40    # R 与 min(G,B) 的差超过该值视为青绿背景
WHITE_LUM = 0.78   # 判定「白」的亮度阈值
# template 变体的填充色。白色：macOS 只取 alpha、忽略 RGB，故填什么都行，
# 填白便于肉眼在预览 PNG 上检查形状。
INK = (255, 255, 255)

Color = tuple[int, int, int, int]


def luminance(px: Color) -> float:
    r, g, b, _ = px
    return (0.299 * r + 0.587 * g + 0.114 * b) / 255.0


def build_mask(src: Path, size: int) -> Image.Image:
    """返回 size x size 的单色 alpha 掩码。形状识别在 WORK 分辨率上做。"""
    work = Image.open(src).convert("RGBA").resize((WORK, WORK), Image.Resampling.LANCZOS)
    px = work.load()
    assert px is not None

    glyph = [[False] * WORK for _ in range(WORK)]
    white = [[False] * WORK for _ in range(WORK)]
    for y in range(WORK):
        for x in range(WORK):
            r, g, b, a = px[x, y]
            if a < 8:
                continue
            if (min(g, b) - r) > TEAL_DELTA:      # 青色底色，不属于图形
                continue
            glyph[y][x] = True
            white[y][x] = luminance(px[x, y]) > WHITE_LUM

    # 找出「被暗部完全包围的白色连通块」= 猫眼。
    # 判据：该白色块的所有边界邻居都在图形内部（不接触任何青色/透明背景）。
    # 猫的白脸直接贴着青色底色，故不会被误判——实测白脸 bg 接触率 0.59、两眼均为 0.00。
    seen = [[False] * WORK for _ in range(WORK)]
    holes = [[False] * WORK for _ in range(WORK)]
    for sy in range(WORK):
        for sx in range(WORK):
            if not white[sy][sx] or seen[sy][sx]:
                continue
            comp: list[tuple[int, int]] = []
            touches_bg = False
            queue = deque([(sy, sx)])
            seen[sy][sx] = True
            while queue:
                y, x = queue.popleft()
                comp.append((y, x))
                for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    ny, nx = y + dy, x + dx
                    if not (0 <= ny < WORK and 0 <= nx < WORK):
                        touches_bg = True
                        continue
                    if glyph[ny][nx]:
                        if white[ny][nx] and not seen[ny][nx]:
                            seen[ny][nx] = True
                            queue.append((ny, nx))
                    else:
                        touches_bg = True
            # 面积上限防止把「整片不接触背景的白脸」误当镂空
            if not touches_bg and len(comp) < WORK * WORK // 8:
                for y, x in comp:
                    holes[y][x] = True

    mask = Image.new("L", (WORK, WORK), 0)
    mp = mask.load()
    assert mp is not None
    for y in range(WORK):
        for x in range(WORK):
            if glyph[y][x] and not holes[y][x]:
                mp[x, y] = 255
    return mask.resize((size, size), Image.Resampling.LANCZOS)


def build_template(mask: Image.Image) -> Image.Image:
    """macOS 变体：纯白剪影，颜色交给系统按 template image 着色。"""
    glyph = Image.new("RGBA", (SIZE, SIZE), (*INK, 0))
    glyph.putalpha(mask)
    return glyph


def color_source(src: Path) -> Image.Image:
    """彩色变体的 RGB 来源：源图**直接**下采样到 COLOR_SIZE。

    刻意与 `build_mask` 的内部工作分辨率（WORK）分开：掩码只关心形状，
    而颜色要尽量贴近源图，多一次缩放只会把细节糊掉。
    """
    return Image.open(src).convert("RGBA").resize(
        (COLOR_SIZE, COLOR_SIZE), Image.Resampling.LANCZOS
    )


def build_color(source: Image.Image, mask: Image.Image) -> Image.Image:
    """Windows / Linux 变体：源图真实颜色 + 同一 alpha 掩码。

    RGB 走**源图**的独立下采样（而不是从 template 复制白色），这样浅色任务栏
    下彩色猫头依然可辨；alpha 仍由 `build_mask` 决定，保证两个平台形状一致。
    透明处 RGB 置 0：LANCZOS 会在轮廓外沿拖出青色残影，若留着这些 RGB，
    合成器会在半透明像素上把它放大成明显色边。
    """
    color = Image.new("RGBA", (COLOR_SIZE, COLOR_SIZE), (0, 0, 0, 0))
    sp = source.load()
    mp = mask.load()
    cp = color.load()
    assert sp is not None and mp is not None and cp is not None
    for y in range(COLOR_SIZE):
        for x in range(COLOR_SIZE):
            alpha = mp[x, y]
            if alpha == 0:
                continue
            r, g, b, _ = sp[x, y]
            cp[x, y] = (r, g, b, alpha)
    return color


def write_raw(path: Path, image: Image.Image, size: int) -> bytes:
    """写出 `.rgba` 原始字节：tray.rs 以 include_bytes! 引用，长度必须恰好 size*size*4。"""
    raw = image.tobytes()
    expected = size * size * 4
    if len(raw) != expected:
        raise SystemExit(f"[gen-tray-icon] {path.name} 字节数异常：{len(raw)} != {expected}")
    path.write_bytes(raw)
    return raw


def verify(
    png_path: Path, raw: bytes, size: int, color_src: Image.Image | None = None
) -> None:
    """回读 PNG，确认它与 .rgba 同源，且颜色符合该变体的约定。

    挡的是 **PNG 往返编码失真**——Rust 侧吃的是 `.rgba`，PNG 只是给人看的预览；
    一旦 Pillow 在存盘时改动了 alpha 或把颜色压成别的值，两边就会悄悄分叉，
    而肉眼只看 PNG 看不出来。注意它**拦不住**「手改产物」：两个文件都在本函数
    之前被重写了，手改只会被静默覆盖（见文件头说明）。

    `color_src` 为 None 表示 template 变体，不透明像素的 RGB 必须是 `INK`；
    否则传入彩色变体的**源图下采样**，要求 RGB 逐像素等于它——这一条同时校验
    「保色下采样」正确、以及 PNG 往返没有动过 RGB。
    """
    img = Image.open(png_path).convert("RGBA")
    if img.size != (size, size):
        raise SystemExit(
            f"[gen-tray-icon] {png_path.name} 尺寸异常：{img.size} != {(size, size)}"
        )
    got = img.tobytes()
    if len(got) != len(raw):
        raise SystemExit(
            f"[gen-tray-icon] {png_path.name} 与 .rgba 长度不一致：{len(got)} != {len(raw)}"
        )
    ref = color_src.tobytes() if color_src is not None else None
    for i in range(0, len(raw), 4):
        if got[i + 3] != raw[i + 3]:
            raise SystemExit(
                f"[gen-tray-icon] {png_path.name} 与 .rgba 的 alpha 在像素 {i // 4} 处不一致 "
                f"（{got[i + 3]} != {raw[i + 3]}）"
            )
        if raw[i + 3] == 0:
            continue
        if (got[i], got[i + 1], got[i + 2]) != (raw[i], raw[i + 1], raw[i + 2]):
            raise SystemExit(
                f"[gen-tray-icon] {png_path.name} 的 RGB 在像素 {i // 4} 处往返失真："
                f"{(got[i], got[i + 1], got[i + 2])} != {(raw[i], raw[i + 1], raw[i + 2])}"
            )
        expected = INK if ref is None else (ref[i], ref[i + 1], ref[i + 2])
        if (raw[i], raw[i + 1], raw[i + 2]) != expected:
            raise SystemExit(
                f"[gen-tray-icon] 像素 {i // 4} 的颜色不符合该变体约定："
                f"{(raw[i], raw[i + 1], raw[i + 2])} != {expected}"
            )


def report(name: str, raw: bytes, size: int, note: str) -> None:
    opaque = sum(1 for i in range(3, len(raw), 4) if raw[i] > 0)
    print(
        f"[gen-tray-icon] 已生成 {size}x{size} {note} "
        f"（不透明像素 {opaque}/{size * size}）"
    )
    print(f"[gen-tray-icon]   {name}.png  {len(raw)} bytes -> {name}.rgba")


def main() -> None:
    src = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "pic" / "logo.png"
    if not src.is_file():
        raise SystemExit(f"[gen-tray-icon] 源图不存在：{src}")

    ICONS.mkdir(parents=True, exist_ok=True)

    # macOS：36x36 纯白 template，RGB 被系统忽略。
    template = build_template(build_mask(src, SIZE))
    template_png = ICONS / "tray-icon-template.png"
    template.save(template_png, "PNG")
    verify(
        template_png,
        write_raw(ICONS / "tray-icon-template.rgba", template, SIZE),
        SIZE,
    )

    # Windows / Linux：32x32 彩色，RGB 原样生效。
    source = color_source(src)
    color = build_color(source, build_mask(src, COLOR_SIZE))
    color_png = ICONS / "tray-icon-color.png"
    color.save(color_png, "PNG")
    verify(
        color_png,
        write_raw(ICONS / "tray-icon-color.rgba", color, COLOR_SIZE),
        COLOR_SIZE,
        color_src=source,
    )

    report("tray-icon-template", template.tobytes(), SIZE, f"单色托盘图标（填充色 RGB{INK}）")
    report("tray-icon-color", color.tobytes(), COLOR_SIZE, "彩色托盘图标（RGB 取自源图）")


if __name__ == "__main__":
    main()
