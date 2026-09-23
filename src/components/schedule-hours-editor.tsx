import { useState } from "react";

import { Plus, X } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useT } from "@/lib/i18n";

/**
 * 某一类定时任务的**小时列表编辑器**：以标签展示当前小时点，可逐个删除或新增。
 *
 * 输入侧即做 0-23 整数校验（非法时不入列并给出可读提示），仅允许合法的整点小时。
 *
 * ## 为什么抽成独立组件（而不是留在某个设置页里）
 *
 * 同一种控件现在有两个使用方：WorkBuddy 设置页的「定时任务」卡片，以及 Trae 设置页
 * 的「自动签到」卡片。**两处各写一遍必然漂移** —— 一边加了重复值提示、另一边没加，
 * 用户的感受就是「同一个功能在两页里行为不同」。校验规则（0-23、去重、排序）属于
 * 规则而不是版式，必须只有一份实现。
 *
 * 组件本身**不认识任何任务**：`id` / `hours` / `onChange` 全部由调用方给，
 * 因此「哪个字段是哪类任务的」这类知识仍留在各自的设置页里。
 */
export function HoursEditor({
  id,
  hours,
  disabled,
  onChange,
}: {
  id: string;
  hours: number[];
  disabled?: boolean;
  onChange: (hours: number[]) => void;
}) {
  const t = useT();
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);

  function add() {
    const raw = draft.trim();
    const value = Number(raw);
    if (raw === "" || !Number.isInteger(value) || value < 0 || value > 23) {
      setError(t("wbSettings.schedule.hourInvalid"));
      return;
    }
    if (hours.includes(value)) {
      setError(t("wbSettings.schedule.hourDuplicate", { hour: value }));
      return;
    }
    setError(null);
    setDraft("");
    onChange([...hours, value].sort((a, b) => a - b));
  }

  return (
    <div className="flex w-full flex-col items-end gap-2 sm:w-auto">
      <div className="flex w-full flex-wrap items-center justify-end gap-1.5">
        {hours.length === 0 ? (
          <span className="text-xs text-muted-foreground">{t("wbSettings.schedule.hourEmpty")}</span>
        ) : (
          hours.map((hour) => (
            <Badge key={hour} variant="secondary" className="gap-1 pr-1 font-mono">
              {String(hour).padStart(2, "0")}:00
              <button
                type="button"
                aria-label={t("wbSettings.schedule.hourRemoveAria", { hour })}
                className="rounded-full p-0.5 text-muted-foreground transition-colors hover:bg-foreground/10 hover:text-foreground disabled:pointer-events-none disabled:opacity-50"
                disabled={disabled}
                onClick={() => onChange(hours.filter((h) => h !== hour))}
              >
                <X className="size-3" />
              </button>
            </Badge>
          ))
        )}
      </div>
      <div className="flex w-full items-center justify-end gap-2">
        <Input
          id={id}
          className="w-full sm:w-24"
          type="number"
          min={0}
          max={23}
          inputMode="numeric"
          placeholder="0-23"
          value={draft}
          disabled={disabled}
          onChange={(e) => {
            setDraft(e.target.value);
            if (error) setError(null);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              add();
            }
          }}
        />
        <Button type="button" size="sm" variant="outline" onClick={add} disabled={disabled}>
          <Plus />
          {t("wbSettings.schedule.hourAdd")}
        </Button>
      </div>
      {error && <p className="text-xs text-destructive">{error}</p>}
    </div>
  );
}
