import { useQuery } from "@tanstack/react-query";
import { Sparkles, Terminal as TerminalIcon } from "lucide-react";
import { presetsList, type AgentPreset } from "@/lib/commands";
import { qk } from "@/query";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useTerminalTabs } from "@/stores/terminal_tabs";

interface Props {
  taskId: string;
}

/** Picker opened by the `+` button at the tab-strip end and the ⌘T
 *  global shortcut. Lists Shell + every configured agent preset; the
 *  user clicks one, we addTab, the strip's `TerminalView` spawns a
 *  fresh PTY. Shell tabs reuse the task view's resolved `shellSpawn`;
 *  agent tabs pass through the same `makeSpawn → agent_launch` pipeline
 *  the auto-launch path uses. Prompt composition lives in Rust
 *  (`compose_first_turn` for the first-ever agent, bootstrap
 *  template thereafter), so picker tabs don't thread any prompt
 *  here — `agent_launch` receives `initial_prompt: null` and
 *  decides the right text based on `initial_prompt_consumed_at`. */
export function NewTabPicker({ taskId }: Props) {
  const openFor = useTerminalTabs((s) => s.newTabOpenForTask);
  const requestNewTab = useTerminalTabs((s) => s.requestNewTab);
  const addTab = useTerminalTabs((s) => s.addTab);
  const open = openFor === taskId;

  const { data: presets = [] } = useQuery<AgentPreset[]>({
    queryKey: qk.agentPresets(),
    queryFn: () => presetsList(),
  });

  const onPickShell = () => {
    addTab({
      id: `${taskId}:shell-${Date.now()}`,
      kind: "shell",
      label: "Shell",
      taskId,
    });
    requestNewTab(null);
  };

  const onPickAgent = (preset: AgentPreset) => {
    addTab({
      id: `${taskId}:agent-${Date.now()}`,
      kind: "agent",
      label: preset.name,
      presetId: preset.id,
      taskId,
    });
    requestNewTab(null);
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => requestNewTab(next ? taskId : null)}
    >
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>New tab</DialogTitle>
        </DialogHeader>
        <div className="flex flex-col gap-1">
          <button
            type="button"
            onClick={onPickShell}
            className="hover:bg-accent flex items-center gap-2 rounded px-2 py-2 text-left text-sm"
          >
            <TerminalIcon size={14} className="text-muted-foreground" />
            <span className="flex-1">Shell</span>
            <span className="text-muted-foreground text-xs">
              worktree shell
            </span>
          </button>
          {presets.map((p) => (
            <button
              key={p.id}
              type="button"
              onClick={() => onPickAgent(p)}
              className="hover:bg-accent flex items-center gap-2 rounded px-2 py-2 text-left text-sm"
            >
              <Sparkles size={14} className="text-muted-foreground" />
              <span className="flex-1">{p.name}</span>
              <span className="text-muted-foreground text-xs">agent</span>
            </button>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}
