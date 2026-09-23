import { useState } from "react";
import { Check, Copy } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { copyText } from "@/lib/clipboard";
import { useT, type Translate } from "@/lib/i18n";
import { cn } from "@/lib/utils";

/** Trae 接入指引覆盖的客户端（Trae 无 region，故比 WorkBuddy 少一个维度）。 */
const TOOLS = [
  { key: "cursor", label: "Cursor" },
  { key: "cline", label: "Cline" },
  { key: "continue", label: "Continue" },
  { key: "cherry", label: "Cherry Studio" },
] as const;

type ToolKey = (typeof TOOLS)[number]["key"];

/**
 * 生成可粘贴的配置片段。
 *
 * 同 `integration-guide.tsx`：`t` 只管字段标签，片段里的键名（`apiProvider`、
 * `apiBase` 等）保持英文 —— 那是外部工具读取的契约。
 */
function snippetFor(t: Translate, tool: ToolKey, baseUrl: string, key: string, model: string): string {
  switch (tool) {
    case "cursor":
      return [
        "Cursor → Settings → Models → OpenAI API Key",
        "",
        `Override OpenAI Base URL: ${baseUrl}`,
        `API Key:  ${key}`,
        `Model:    ${model}`,
      ].join("\n");
    case "cline":
      return JSON.stringify(
        {
          apiProvider: "openai",
          openAiBaseUrl: baseUrl,
          openAiApiKey: key,
          openAiModelId: model,
        },
        null,
        2,
      );
    case "continue":
      return [
        "models:",
        "  - name: Trae",
        "    provider: openai",
        `    model: ${model}`,
        `    apiBase: ${baseUrl}`,
        `    apiKey: ${key}`,
      ].join("\n");
    case "cherry":
      return [
        `${t("trae.gateway.guide.snippet.apiBase")}: ${baseUrl}`,
        `${t("trae.gateway.guide.snippet.apiKey")}: ${key}`,
        `${t("trae.gateway.guide.snippet.model")}: ${model}`,
      ].join("\n");
    default:
      return "";
  }
}

/**
 * Trae 接入指引（对齐 WorkBuddy `gateway/integration-guide.tsx` 骨架）。
 *
 * ## 与 WorkBuddy 版的差别：**单条 Base URL**，没有「按版本」双条目
 *
 * WorkBuddy 版顶部有一排 region Tabs（国内版 / 国际版各自一个 Base URL）；
 * Trae 没有 region —— 一个网关、一条上游、一个 Base URL，因此这里只展示一条。
 * 把 `REGIONS.map` 搬过来会造出一个「切了也没差别」的假维度。
 *
 * `keyPrefix` 由页面从网关状态（`apiKeyPrefix`，最近一把未吊销 Key 的前缀）传入；
 * 为空时给出「请先在上方创建 Key」的占位，而不是伪造一段看起来可用的 Key。
 */
export function TraeIntegrationGuide({
  baseUrl,
  model,
  keyPrefix,
  className,
}: {
  baseUrl: string;
  model: string;
  keyPrefix?: string;
  className?: string;
}) {
  const t = useT();
  const [tool, setTool] = useState<ToolKey>("cursor");
  const [copied, setCopied] = useState(false);

  const key = keyPrefix ? `${keyPrefix}…` : t("trae.gateway.guide.noKey");
  const snippet = snippetFor(t, tool, baseUrl, key, model);

  async function onCopy() {
    await copyText(snippet, t("trae.gateway.guide.copied"));
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }

  return (
    <Card className={cn("gap-0 py-0", className)}>
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border/60 px-5 py-3">
        <span className="text-sm font-semibold">{t("trae.gateway.guide.title")}</span>
        <Button variant="outline" size="sm" onClick={() => void onCopy()}>
          {copied ? <Check /> : <Copy />}
          {t("trae.gateway.guide.copy")}
        </Button>
      </div>

      <Tabs value={tool} onValueChange={(value) => setTool(value as ToolKey)}>
        <div className="border-b border-border/60 px-5 py-2">
          <TabsList className="flex-wrap">
            {TOOLS.map((item) => (
              <TabsTrigger key={item.key} value={item.key}>
                {item.label}
              </TabsTrigger>
            ))}
          </TabsList>
        </div>
        <div className="px-5 py-4">
          <pre className="min-w-0 overflow-x-auto rounded-lg border border-border bg-muted/40 p-4 font-mono text-xs leading-6">
            {snippet}
          </pre>
          <p className="mt-2 text-xs text-muted-foreground">
            {t("trae.gateway.guide.streamNote")}
          </p>
        </div>
      </Tabs>
    </Card>
  );
}
