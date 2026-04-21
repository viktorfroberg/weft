import { Sparkles, User } from "lucide-react";
import { toast } from "sonner";
import { usePrefs } from "@/stores/prefs";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Card } from "./Card";

export function WorkflowTab() {
  const autoLaunch = usePrefs((s) => s.autoLaunchAgentOnTickets);
  const setAutoLaunch = usePrefs((s) => s.setAutoLaunchAgentOnTickets);
  const autoRenameTasks = usePrefs((s) => s.autoRenameTasks);
  const setAutoRenameTasks = usePrefs((s) => s.setAutoRenameTasks);
  const userName = usePrefs((s) => s.userName);
  const setUserName = usePrefs((s) => s.setUserName);

  return (
    <>
      <Card
        title="You"
        description="Used in the Home greeting and anywhere else weft addresses you directly. Local to this device — never sent to any provider."
        Icon={User}
      >
        <div className="flex items-center gap-3">
          <label htmlFor="user-name" className="text-sm">
            Display name
          </label>
          <Input
            id="user-name"
            value={userName}
            onChange={(e) => setUserName(e.target.value)}
            placeholder="e.g. Viktor"
            className="h-8 max-w-xs text-sm"
          />
        </div>
      </Card>

      <Card
        title="Behavior"
        description="Opt-in toggles that change how weft responds to common actions."
        Icon={Sparkles}
      >
        <ToggleRow
          title="Auto-launch agent on ticket-linked tasks"
          description={
            <>
              When you create a task with one or more linked tickets, spawn the
              default agent preset automatically so it's ready with the
              compose-card prompt + ticket summary already in its first turn.
              Empty-session tasks still require pressing Launch (⌘L).
            </>
          }
          checked={autoLaunch}
          onCheckedChange={(v) => {
            setAutoLaunch(v);
            toast.success(v ? "Auto-launch enabled" : "Auto-launch disabled");
          }}
        />
        <ToggleRow
          title="Auto-rename tasks with Claude"
          description={
            <>
              After creating a task, weft runs{" "}
              <code className="font-mono">claude -p --model haiku</code> in the
              background to turn your prompt into a short sidebar label. Uses
              your existing Claude Code install — no API key needed. Turn off
              to stick with the first-line heuristic. You can always rename a
              task manually from the header.
            </>
          }
          checked={autoRenameTasks}
          onCheckedChange={(v) => {
            setAutoRenameTasks(v);
            toast.success(
              v ? "Auto-rename enabled" : "Auto-rename disabled",
            );
          }}
        />
      </Card>
    </>
  );
}

function ToggleRow({
  title,
  description,
  checked,
  onCheckedChange,
}: {
  title: string;
  description: React.ReactNode;
  checked: boolean;
  onCheckedChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-start justify-between gap-4 py-1">
      <div className="min-w-0">
        <div className="text-sm">{title}</div>
        <p className="text-muted-foreground mt-0.5 text-xs">{description}</p>
      </div>
      <Switch
        checked={checked}
        onCheckedChange={onCheckedChange}
        className="mt-1 shrink-0"
      />
    </div>
  );
}
