import { useEffect, useState } from "react";
import {
  Bot,
  Layers,
  Palette,
  Plug,
  Sparkles,
  Wrench,
} from "lucide-react";
import { appInfo, type AppInfo } from "@/lib/commands";
import { useUi } from "@/stores/ui";
import { AppearanceTab } from "./AppearanceTab";
import { IntegrationsTab } from "./IntegrationsTab";
import { PresetsTab } from "./PresetsTab";
import { RepoGroupsTab } from "./RepoGroupsTab";
import { WorkflowTab } from "./WorkflowTab";
import { AdvancedTab } from "./AdvancedTab";

type Tab =
  | "appearance"
  | "repo-groups"
  | "integrations"
  | "agents"
  | "workflow"
  | "advanced";

interface TabMeta {
  id: Tab;
  label: string;
  Icon: typeof Palette;
}

const TABS: TabMeta[] = [
  { id: "appearance", label: "Appearance", Icon: Palette },
  { id: "repo-groups", label: "Repo groups", Icon: Layers },
  { id: "integrations", label: "Integrations", Icon: Plug },
  { id: "agents", label: "Agents", Icon: Bot },
  { id: "workflow", label: "Workflow", Icon: Sparkles },
  { id: "advanced", label: "Advanced", Icon: Wrench },
];

/**
 * Settings root — app-level prefs only. Per-repo configuration lives on
 * `/projects/:projectId` (its own route), not in here. Each tab panel is
 * its own sibling file so adding a new setting category means adding ONE
 * new file + one line in TABS, not editing a 650-line monolith.
 */
const VALID_TABS = new Set<Tab>([
  "appearance",
  "repo-groups",
  "integrations",
  "agents",
  "workflow",
  "advanced",
]);

export function SettingsView() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const pendingTab = useUi((s) => s.pendingSettingsTab);
  const setPendingTab = useUi((s) => s.setPendingSettingsTab);
  const [tab, setTab] = useState<Tab>(() =>
    pendingTab && VALID_TABS.has(pendingTab as Tab)
      ? (pendingTab as Tab)
      : "appearance",
  );

  useEffect(() => {
    appInfo().then(setInfo).catch(() => {});
  }, []);

  // Deep-link consumer: when a caller sets `pendingSettingsTab` *after*
  // SettingsView is already mounted (re-entering /settings while it's
  // alive in the route cache), switch to it and clear the field.
  useEffect(() => {
    if (pendingTab && VALID_TABS.has(pendingTab as Tab)) {
      setTab(pendingTab as Tab);
      setPendingTab(null);
    }
  }, [pendingTab, setPendingTab]);

  return (
    <div className="grid h-full grid-cols-[200px_1fr] overflow-hidden">
      <nav className="border-border flex flex-col overflow-y-auto border-r">
        <div className="flex flex-1 flex-col gap-0.5 p-3">
          {TABS.map(({ id, label, Icon }) => {
            const active = tab === id;
            return (
              <button
                key={id}
                type="button"
                onClick={() => setTab(id)}
                className={`flex items-center gap-2 rounded px-2 py-1.5 text-left text-sm transition-colors ${
                  active
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground hover:bg-accent hover:text-foreground"
                }`}
              >
                <Icon size={14} />
                <span>{label}</span>
              </button>
            );
          })}
        </div>
        {info && (
          <div className="border-border border-t p-2">
            <span className="text-muted-foreground flex h-7 items-center px-2.5 font-mono text-[10px]">
              v{info.version}
            </span>
          </div>
        )}
      </nav>

      {/* `key` forces a mount-animation on tab change, matching the
          app-level route fade. */}
      <div
        key={tab}
        className="animate-in fade-in flex-1 overflow-y-auto p-6 duration-100"
      >
        <div className="mx-auto max-w-2xl space-y-6">
          {tab === "appearance" && <AppearanceTab />}
          {tab === "repo-groups" && <RepoGroupsTab />}
          {tab === "integrations" && <IntegrationsTab />}
          {tab === "agents" && <PresetsTab />}
          {tab === "workflow" && <WorkflowTab />}
          {tab === "advanced" && <AdvancedTab info={info} />}
        </div>
      </div>
    </div>
  );
}
