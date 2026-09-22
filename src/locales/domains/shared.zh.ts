/**
 * 文案域：**跨切面共享文案**（中文，键的权威之一）。
 *
 * 归属文件（`lib/` 与 `stores/` 里的**用户可见**文案，以及两个产品共用的展示组件）：
 * `src/lib/region.ts`、`src/lib/trae-variant-status.ts`、`src/lib/trae-client.ts`、
 * `src/lib/gateway.ts`、`src/lib/trae-gateway.ts`、`src/lib/use-cached-resource.ts`、
 * `src/lib/types.ts`、`src/lib/trae-types.ts`、`src/lib/demo-mode.ts`、
 * `src/lib/update.ts`、`src/lib/clipboard.ts`、`src/lib/stacked-bar-visuals.ts`、
 * `src/stores/resources.ts`、`src/stores/accounts.ts`、`src/stores/gateway.ts`、
 * `src/components/product-marks.tsx`、`src/components/region-bar.tsx`、
 * `src/components/demo-action.tsx`
 *
 * ⚠️ 这些文件大多是**模块级常量表**（如「状态 → 显示标签」的映射）。必须把
 * **键**存进表里（`labelKey: "shared.status.x"`），渲染处再 `t(...)` ——
 * 存中文会让整张表在语言切换时失效（表在模块加载时就定型了）。
 *
 * 键前缀：`shared.`
 */
export const zh = {} as const;
