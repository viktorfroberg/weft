import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { Bot, Pencil, Plus, Star, Trash2 } from "lucide-react";
import {
  presetDelete,
  presetSetDefault,
  presetsList,
  type AgentPreset,
} from "@/lib/commands";
import { qk } from "@/query";
import { useConfirm } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import { Card } from "./Card";
import { EditPresetDialog } from "./EditPresetDialog";

/**
 * CRUD for `agent_presets`. The seed Claude Code row is not specially
 * protected — the repo's delete rules (reject last row, promote next on
 * default-delete) keep the DB in a valid state regardless.
 */
export function PresetsTab() {
  const { data: presets = [] } = useQuery<AgentPreset[]>({
    queryKey: qk.agentPresets(),
    queryFn: () => presetsList(),
  });
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editing, setEditing] = useState<AgentPreset | null>(null);

  const openCreate = () => {
    setEditing(null);
    setDialogOpen(true);
  };
  const openEdit = (preset: AgentPreset) => {
    setEditing(preset);
    setDialogOpen(true);
  };

  return (
    <Card
      title="Agents"
      description="Presets for spawning CLI agents in a task's PTY. Pick a default here; the compose card offers any non-default preset via the agent picker. Args / env support launch-time token substitution."
      Icon={Bot}
    >
      {presets.length === 0 ? (
        <p className="text-muted-foreground text-xs">No presets yet.</p>
      ) : (
        <ul className="space-y-2">
          {presets.map((p) => (
            <PresetRow
              key={p.id}
              preset={p}
              onEdit={() => openEdit(p)}
              isOnlyRow={presets.length === 1}
            />
          ))}
        </ul>
      )}
      <div className="mt-3 flex justify-end">
        <Button
          size="sm"
          variant="ghost"
          onClick={openCreate}
          className="h-7 gap-1 text-xs"
        >
          <Plus size={12} />
          New preset
        </Button>
      </div>
      <EditPresetDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        preset={editing}
      />
    </Card>
  );
}

function PresetRow({
  preset,
  onEdit,
  isOnlyRow,
}: {
  preset: AgentPreset;
  onEdit: () => void;
  isOnlyRow: boolean;
}) {
  const confirm = useConfirm();

  const onDelete = async () => {
    if (isOnlyRow) {
      toast.error("Can't delete the last preset", {
        description: "Create another one first.",
      });
      return;
    }
    const ok = await confirm({
      title: `Delete preset "${preset.name}"?`,
      description: preset.is_default
        ? "This is the current default. Another preset will be promoted automatically."
        : "Tasks using this preset at launch time fall back to the default.",
      confirmText: "Delete",
      destructive: true,
    });
    if (!ok) return;
    try {
      await presetDelete(preset.id);
      toast.success(`Deleted “${preset.name}”`);
    } catch (e) {
      toast.error("Delete failed", { description: String(e) });
    }
  };

  const onSetDefault = async () => {
    try {
      await presetSetDefault(preset.id);
      toast.success(`“${preset.name}” is now the default`);
    } catch (e) {
      toast.error("Couldn't set default", { description: String(e) });
    }
  };

  return (
    <li className="border-border bg-card flex items-center gap-2 rounded-md border px-3 py-2">
      <div className="flex min-w-0 flex-1 items-center gap-2">
        <span className="truncate text-sm font-medium">{preset.name}</span>
        <span className="text-muted-foreground truncate font-mono text-[10px]">
          {preset.command}
        </span>
        {preset.is_default && (
          <span className="bg-accent text-accent-foreground rounded px-1.5 py-0.5 text-[10px] font-medium">
            default
          </span>
        )}
      </div>
      {!preset.is_default && (
        <Button
          size="sm"
          variant="ghost"
          onClick={onSetDefault}
          className="text-muted-foreground hover:text-foreground h-7 w-7 p-0"
          title="Set as default"
        >
          <Star size={12} />
        </Button>
      )}
      <Button
        size="sm"
        variant="ghost"
        onClick={onEdit}
        className="text-muted-foreground hover:text-foreground h-7 w-7 p-0"
        title="Edit preset"
      >
        <Pencil size={12} />
      </Button>
      <Button
        size="sm"
        variant="ghost"
        onClick={onDelete}
        className="text-muted-foreground hover:text-destructive h-7 w-7 p-0"
        title="Delete preset"
      >
        <Trash2 size={12} />
      </Button>
    </li>
  );
}
