import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import CustomSelect from "@/components/CustomSelect";
import RuntimeSelector from "@/components/RuntimeSelector";
import { useToast } from "@/components/Toast";
import {
  normalizeChatQueueResponseMode,
  PERMISSION_MODES,
  type AppConfig,
  type ChatQueueResponseMode,
} from "@/config/types";
import type { RuntimeDetections } from "../../../../shared/types/runtime";
import {
  AGENT_RUNTIME_DISTRIBUTION_POLICY,
  defaultIntegratedRuntimeType,
} from "../../../../shared/integrated-runtimes/distribution-policy";
import { normalizeDefaultSessionPermissionMode } from "../../../../shared/runtimeBirthFields";
import { DefaultModelSetting } from "./DefaultModelSetting";

interface AgentFunctionSettingsProps {
  config: Pick<AppConfig, "defaultIntegratedRuntime" | "chatQueueResponseMode">
    & Partial<Pick<AppConfig, "defaultPermissionMode" | "defaultModelSelection">>;
  updateConfig: (patch: Partial<AppConfig>) => Promise<void>;
}

export default function AgentFunctionSettings({
  config,
  updateConfig,
}: AgentFunctionSettingsProps) {
  const { t: tSettings } = useTranslation("settings");
  const { t: tChat } = useTranslation("chat");
  const toast = useToast();
  const [savingPermissionMode, setSavingPermissionMode] = useState(false);
  const permissionMode = normalizeDefaultSessionPermissionMode(config.defaultPermissionMode);
  const permissionModeLabel = tSettings("general.defaultSessionModeTitle");
  const defaultRuntime = defaultIntegratedRuntimeType(
    AGENT_RUNTIME_DISTRIBUTION_POLICY,
    config.defaultIntegratedRuntime,
  );

  async function savePermissionMode(value: string) {
    const next = normalizeDefaultSessionPermissionMode(value);
    if (next === permissionMode) return;
    setSavingPermissionMode(true);
    try {
      await updateConfig({ defaultPermissionMode: next });
    } catch {
      toast.error(tSettings("general.saveFailedRetry"));
    } finally {
      setSavingPermissionMode(false);
    }
  }
  const [detections, setDetections] = useState<RuntimeDetections>({
    builtin: { installed: true },
    dsh: { installed: false },
    "claude-code": { installed: false },
    codex: { installed: false },
  });
  useEffect(() => {
    let active = true;
    void invoke<RuntimeDetections>("cmd_detect_runtimes")
      .then((result) => {
        if (active) setDetections(result);
      })
      .catch((error) =>
        console.error("[runtime] Default environment detection failed:", error),
      );
    return () => {
      active = false;
    };
  }, []);
  return (
    <div className="rounded-xl border border-[var(--line)] bg-[var(--paper-elevated)] p-5">
      <h3 className="text-base font-medium text-[var(--ink)]">
        {tSettings("general.agentFeaturesTitle")}
      </h3>
      <div className="mt-4 flex items-center justify-between gap-4">
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium text-[var(--ink)]">
            {tSettings("general.defaultRuntimeTitle")}
          </p>
          <p className="text-xs text-[var(--ink-muted)]">
            {tSettings("general.defaultRuntimeDescription")}
          </p>
        </div>
        <div className="w-72 max-w-[45%] shrink-0">
          <RuntimeSelector
            value={defaultRuntime}
            detections={detections}
            variant="panel"
            integratedOnly
            onChange={(runtime) =>
              void updateConfig({
                defaultIntegratedRuntime:
                  runtime === "dsh" ? "dsh" : "claude-agent-sdk",
              })
            }
          />
        </div>
      </div>
      <DefaultModelSetting selection={config.defaultModelSelection} runtime={defaultRuntime} updateConfig={updateConfig} />
      <div className="mt-4 flex items-center justify-between gap-4 border-t border-[var(--line)] pt-4">
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium text-[var(--ink)]">{permissionModeLabel}</p>
          <p className="text-xs text-[var(--ink-muted)]">
            {tSettings("general.defaultSessionModeDescription")}
            {permissionMode === "plan" && ` ${tSettings("general.defaultSessionModeDshPlan")}`}
          </p>
        </div>
        <CustomSelect
          ariaLabel={permissionModeLabel}
          value={permissionMode}
          options={PERMISSION_MODES.map(({ value }) => ({
            value,
            label: tChat(`input.permissionModes.${value}.label`),
          }))}
          size="md"
          className="w-72 max-w-[45%] shrink-0"
          disabled={savingPermissionMode}
          onChange={(value) => void savePermissionMode(value)}
        />
      </div>
      <div className="mt-4 flex items-center justify-between gap-4 border-t border-[var(--line)] pt-4">
        <div className="flex-1">
          <p className="text-sm font-medium text-[var(--ink)]">
            {tSettings("general.queueModeTitle")}
          </p>
          <p className="text-xs text-[var(--ink-muted)]">
            {normalizeChatQueueResponseMode(config.chatQueueResponseMode) ===
            "turn"
              ? tSettings("general.queueTurnDescription")
              : tSettings("general.queueRealtimeDescription")}
          </p>
        </div>
        <div className="flex shrink-0 gap-0.5 rounded-full bg-[var(--paper-inset)] p-0.5">
          {(
            [
              {
                value: "realtime",
                label: tSettings("general.queueRealtime"),
              },
              {
                value: "turn",
                label: tSettings("general.queueTurn"),
              },
            ] as const satisfies ReadonlyArray<{
              value: ChatQueueResponseMode;
              label: string;
            }>
          ).map((opt) => {
            const active =
              normalizeChatQueueResponseMode(config.chatQueueResponseMode) ===
              opt.value;
            return (
              <button
                key={opt.value}
                onClick={() =>
                  void updateConfig({
                    chatQueueResponseMode: opt.value,
                  })
                }
                className={`rounded-full px-3 py-1 text-xs font-medium transition-all ${
                  active
                    ? "bg-[var(--paper-elevated)] text-[var(--ink)] shadow-sm"
                    : "text-[var(--ink-muted)] hover:text-[var(--ink-secondary)]"
                }`}
              >
                {opt.label}
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
