import {
  presetDefault,
  type AgentPreset,
} from "@/lib/commands";
import { useTerminalTabs } from "@/stores/terminal_tabs";

/** Shared "launch the default agent for a task" action.
 *
 *  Used from the Toolbar's ⌘L shortcut, the Compose card's
 *  auto-launch on task creation, and the ⌘T new-tab picker. All
 *  three spawn an agent tab for the task's default preset.
 *
 *  Prompt composition moved into Rust in v1.1. Here we just push a
 *  tab; `TerminalTabStrip.makeSpawn` calls `agent_launch` with
 *  `initial_prompt: null` and the Rust side decides whether to
 *  compose a first-turn message (task's `initial_prompt` is still
 *  unconsumed) or expand the preset's `bootstrap_prompt_template`
 *  (everything else). One code path, one source of truth.
 *
 *  Side effects:
 *  - Fetches the default `AgentPreset` fresh each call (cheap SELECT).
 *  - If none is configured, resolves to `null` so the caller can decide
 *    whether to toast or alert.
 *  - Otherwise appends a new agent tab; the tab spawn fn fires
 *    `agent_launch` on mount. */
export async function launchDefaultAgent(taskId: string): Promise<AgentPreset | null> {
  const preset = await presetDefault();
  if (!preset) return null;
  useTerminalTabs.getState().addTab({
    id: `${taskId}:agent-${Date.now()}`,
    kind: "agent",
    label: preset.name,
    presetId: preset.id,
    taskId,
  });
  return preset;
}
