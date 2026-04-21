import { useEffect, useState } from "react";
import { Eye, EyeOff, Loader2 } from "lucide-react";
import { toast } from "sonner";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import {
  presetCreate,
  presetUpdate,
  type AgentPreset,
  type BootstrapDelivery,
} from "@/lib/commands";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Present = edit mode; absent = create mode. */
  preset?: AgentPreset | null;
}

/** Editor dialog for `agent_presets`. Used for both create and edit;
 *  mode is driven by the presence of the `preset` prop. Args / env are
 *  edited as raw JSON (matches the on-disk storage shape and the token
 *  syntax — `{slug}`, `{each_path:--add-dir}`, `{prompt}`, `{bootstrap}` —
 *  that agent_launch.rs substitutes). Env values are masked by default
 *  because they can carry API keys — masking is preview-only, the
 *  underlying JSON isn't altered. */
export function EditPresetDialog({ open, onOpenChange, preset }: Props) {
  const editing = !!preset;
  const [name, setName] = useState("");
  const [command, setCommand] = useState("");
  const [argsJson, setArgsJson] = useState("[]");
  const [envJson, setEnvJson] = useState("{}");
  const [bootstrap, setBootstrap] = useState("");
  const [delivery, setDelivery] = useState<BootstrapDelivery | "">("");
  const [revealEnv, setRevealEnv] = useState(false);
  const [saving, setSaving] = useState(false);
  const [argsErr, setArgsErr] = useState<string | null>(null);
  const [envErr, setEnvErr] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    if (preset) {
      setName(preset.name);
      setCommand(preset.command);
      setArgsJson(preset.args_json);
      setEnvJson(preset.env_json);
      setBootstrap(preset.bootstrap_prompt_template ?? "");
      setDelivery(preset.bootstrap_delivery ?? "");
    } else {
      setName("");
      setCommand("");
      setArgsJson("[]");
      setEnvJson("{}");
      setBootstrap("");
      setDelivery("");
    }
    setRevealEnv(false);
    setArgsErr(null);
    setEnvErr(null);
  }, [open, preset]);

  const validate = (): boolean => {
    let ok = true;
    try {
      const parsed = JSON.parse(argsJson);
      if (!Array.isArray(parsed) || parsed.some((v) => typeof v !== "string")) {
        throw new Error("must be an array of strings");
      }
      setArgsErr(null);
    } catch (e) {
      setArgsErr(`args must be a JSON array of strings (${String(e)})`);
      ok = false;
    }
    try {
      const parsed = JSON.parse(envJson);
      if (
        typeof parsed !== "object" ||
        parsed === null ||
        Array.isArray(parsed) ||
        Object.values(parsed).some((v) => typeof v !== "string")
      ) {
        throw new Error("must be a JSON object of string→string");
      }
      setEnvErr(null);
    } catch (e) {
      setEnvErr(`env must be a JSON object of string→string (${String(e)})`);
      ok = false;
    }
    if (!name.trim() || !command.trim()) ok = false;
    return ok;
  };

  const onSave = async () => {
    if (!validate()) return;
    setSaving(true);
    const bootstrapTrim = bootstrap.trim() || null;
    const deliveryValue = bootstrapTrim ? (delivery || "argv") : null;
    try {
      if (preset) {
        await presetUpdate(preset.id, {
          name: name.trim(),
          command: command.trim(),
          args_json: argsJson,
          env_json: envJson,
          sort_order: preset.sort_order,
          bootstrap_prompt_template: bootstrapTrim,
          bootstrap_delivery: deliveryValue,
        });
        toast.success(`Saved “${name.trim()}”`);
      } else {
        await presetCreate({
          name: name.trim(),
          command: command.trim(),
          args_json: argsJson,
          env_json: envJson,
          bootstrap_prompt_template: bootstrapTrim,
          bootstrap_delivery: deliveryValue,
        });
        toast.success(`Created “${name.trim()}”`);
      }
      onOpenChange(false);
    } catch (e) {
      toast.error(preset ? "Save failed" : "Create failed", {
        description: String(e),
      });
    } finally {
      setSaving(false);
    }
  };

  const maskedEnv = envJson
    .replace(/"([^"\\]*(?:\\.[^"\\]*)*)"\s*:\s*"([^"\\]*(?:\\.[^"\\]*)*)"/g,
      (_m, k: string) => `"${k}": "•••"`);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{editing ? "Edit preset" : "New preset"}</DialogTitle>
          <DialogDescription>
            Template for spawning an agent in a task's PTY. Args / env
            support token substitution (<code className="font-mono text-[10px]">{"{slug}"}</code>,
            {" "}<code className="font-mono text-[10px]">{"{each_path:--add-dir}"}</code>,
            {" "}<code className="font-mono text-[10px]">{"{prompt}"}</code>,
            {" "}<code className="font-mono text-[10px]">{"{bootstrap}"}</code>) resolved at launch.
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-4">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <Label htmlFor="preset-name">Name</Label>
              <Input
                id="preset-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Claude Code"
                autoFocus
              />
            </div>
            <div>
              <Label htmlFor="preset-command">Command</Label>
              <Input
                id="preset-command"
                value={command}
                onChange={(e) => setCommand(e.target.value)}
                placeholder="claude"
                className="font-mono"
              />
            </div>
          </div>

          <div>
            <Label htmlFor="preset-args">Args (JSON array)</Label>
            <Textarea
              id="preset-args"
              value={argsJson}
              onChange={(e) => setArgsJson(e.target.value)}
              className="font-mono text-xs"
              rows={3}
            />
            {argsErr && (
              <p className="text-destructive mt-1 text-[11px]">{argsErr}</p>
            )}
          </div>

          <div>
            <div className="mb-1 flex items-center justify-between">
              <Label htmlFor="preset-env">Env (JSON object)</Label>
              <button
                type="button"
                onClick={() => setRevealEnv((v) => !v)}
                className="text-muted-foreground hover:text-foreground flex items-center gap-1 text-[11px]"
                title={revealEnv ? "Hide values" : "Reveal values"}
              >
                {revealEnv ? <EyeOff size={11} /> : <Eye size={11} />}
                {revealEnv ? "Hide" : "Reveal"}
              </button>
            </div>
            {revealEnv ? (
              <Textarea
                id="preset-env"
                value={envJson}
                onChange={(e) => setEnvJson(e.target.value)}
                className="font-mono text-xs"
                rows={3}
              />
            ) : (
              <Textarea
                id="preset-env"
                value={maskedEnv}
                readOnly
                className="text-muted-foreground font-mono text-xs"
                rows={3}
              />
            )}
            {envErr && (
              <p className="text-destructive mt-1 text-[11px]">{envErr}</p>
            )}
            {!revealEnv && envJson !== "{}" && (
              <p className="text-muted-foreground mt-1 text-[11px]">
                Values are masked. Click Reveal to edit.
              </p>
            )}
          </div>

          <div>
            <Label htmlFor="preset-bootstrap">
              Bootstrap prompt template (optional)
            </Label>
            <Textarea
              id="preset-bootstrap"
              value={bootstrap}
              onChange={(e) => setBootstrap(e.target.value)}
              placeholder="Orientation for a second agent joining mid-task…"
              className="text-xs"
              rows={3}
            />
            {bootstrap.trim() && (
              <div className="mt-2 flex items-center gap-4">
                <span className="text-muted-foreground text-[11px]">
                  Delivery
                </span>
                <label className="flex items-center gap-1.5 text-[11px]">
                  <input
                    type="radio"
                    name="delivery"
                    checked={delivery === "argv" || delivery === ""}
                    onChange={() => setDelivery("argv")}
                  />
                  argv ({"{bootstrap}"} token)
                </label>
                <label className="flex items-center gap-1.5 text-[11px]">
                  <input
                    type="radio"
                    name="delivery"
                    checked={delivery === "append_system_prompt"}
                    onChange={() => setDelivery("append_system_prompt")}
                  />
                  --append-system-prompt (Claude)
                </label>
              </div>
            )}
          </div>
        </div>

        <DialogFooter>
          <Button
            variant="ghost"
            onClick={() => onOpenChange(false)}
            disabled={saving}
          >
            Cancel
          </Button>
          <Button onClick={onSave} disabled={saving}>
            {saving && <Loader2 size={12} className="mr-1 animate-spin" />}
            {editing ? "Save" : "Create preset"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
