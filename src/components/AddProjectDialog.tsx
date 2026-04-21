import { useState } from "react";
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
import {
  projectCreate,
  projectLinksDetectPreset,
  projectLinksPresetApply,
} from "@/lib/commands";
import { useProjectLinkPresets } from "@/stores/project_links";
import {
  gitDefaultBranch,
  gitIsRepo,
  pickDirectory,
} from "@/lib/dialog";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

// Tailwind palette sample — Phase 3 keeps color a plain text field.
// Expand to a picker in polish.
const SUGGESTED_COLORS = [
  "#c084fc", // purple
  "#60a5fa", // blue
  "#2dd4bf", // teal
  "#fb923c", // orange
  "#f472b6", // pink
  "#facc15", // yellow
];

function basename(path: string): string {
  return path.split("/").filter(Boolean).pop() ?? path;
}

function PresetOption({
  active,
  onClick,
  label,
  title,
}: {
  active: boolean;
  onClick: () => void;
  label: string;
  title?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      className={`flex-1 rounded px-2.5 py-1 text-xs transition-all ${
        active
          ? "bg-background text-foreground shadow-sm"
          : "text-muted-foreground hover:text-foreground"
      }`}
    >
      {label}
    </button>
  );
}

export function AddProjectDialog({ open, onOpenChange }: Props) {
  const [stage, setStage] = useState<"pick" | "confirm">("pick");
  const [path, setPath] = useState<string>("");
  const [name, setName] = useState<string>("");
  const [defaultBranch, setDefaultBranch] = useState<string>("main");
  const [color, setColor] = useState<string>(SUGGESTED_COLORS[0]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  /** null = "Leave cold" (no preset applied). `""` is not a valid preset id.
   *  Selected preset is applied AFTER the project row lands. */
  const [presetId, setPresetId] = useState<string | null>(null);
  const { data: presets = [] } = useProjectLinkPresets();

  const reset = () => {
    setStage("pick");
    setPath("");
    setName("");
    setDefaultBranch("main");
    setColor(SUGGESTED_COLORS[0]);
    setError(null);
    setBusy(false);
    setPresetId(null);
  };

  const onPickPath = async () => {
    setError(null);
    const picked = await pickDirectory();
    if (!picked) return;
    setBusy(true);
    try {
      const isRepo = await gitIsRepo(picked);
      if (!isRepo) {
        setError(`${picked} is not a git repository.`);
        setBusy(false);
        return;
      }
      const branch = await gitDefaultBranch(picked).catch(() => "main");
      setPath(picked);
      setName(basename(picked));
      setDefaultBranch(branch);
      // Probe for a matching warm-worktree preset. Non-fatal — if the
      // repo is exotic the user can still pick Custom or Leave cold.
      try {
        const detected = await projectLinksDetectPreset(picked);
        setPresetId(detected);
      } catch {
        setPresetId(null);
      }
      setStage("confirm");
    } catch (e) {
      // `git` missing from PATH is the most common first-run failure on
      // a fresh macOS install. Translate the generic ENOENT into a
      // specific, actionable message before falling back.
      const msg = String(e);
      const looksLikeNoGit =
        /No such file or directory|command not found|cannot find|not found/i.test(msg) &&
        /git/i.test(msg);
      if (looksLikeNoGit) {
        setError(
          "git isn't installed or isn't on PATH. Install the Xcode Command Line Tools with `xcode-select --install`, or install git via Homebrew (`brew install git`), then try again.",
        );
      } else {
        setError(msg);
      }
    } finally {
      setBusy(false);
    }
  };

  const onConfirm = async () => {
    setError(null);
    setBusy(true);
    try {
      const created = await projectCreate({
        name: name.trim(),
        main_repo_path: path,
        default_branch: defaultBranch.trim() || "main",
        color,
      });
      // Apply the warm-worktree preset if one was selected. Done after
      // project_create so we have the real project id; failure here is
      // non-fatal — the project exists, the user can set links later
      // from Settings → Projects.
      if (presetId) {
        try {
          await projectLinksPresetApply(created.id, presetId);
        } catch (e) {
          console.warn("preset apply failed (non-fatal)", e);
        }
      }
      onOpenChange(false);
      reset();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const dirty = stage === "confirm" && !busy;
  const requestClose = (next: boolean) => {
    if (!next && dirty) {
      if (!confirm("Discard this repo? You'll need to pick the directory again.")) {
        return;
      }
    }
    onOpenChange(next);
    if (!next) reset();
  };

  return (
    <Dialog open={open} onOpenChange={requestClose}>
      <DialogContent
        className="sm:max-w-md"
        onInteractOutside={(e) => {
          if (dirty) e.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>Add a repo</DialogTitle>
          <DialogDescription>
            Register a git repository so tasks can include it.
          </DialogDescription>
        </DialogHeader>

        {stage === "pick" && (
          <div className="space-y-3">
            <p className="text-foreground text-sm">
              Pick a directory that is a git repository. weft will detect its
              default branch automatically.
            </p>
            {error && (
              <p className="text-destructive text-sm" role="alert">
                {error}
              </p>
            )}
            <Button onClick={onPickPath} disabled={busy}>
              {busy ? "Checking…" : "Choose directory…"}
            </Button>
          </div>
        )}

        {stage === "confirm" && (
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="proj-path">Path</Label>
              <Input
                id="proj-path"
                value={path}
                readOnly
                className="text-muted-foreground font-mono text-xs"
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="proj-name">Display name</Label>
              <Input
                id="proj-name"
                autoFocus
                value={name}
                onChange={(e) => setName(e.target.value)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="proj-branch">Default branch</Label>
              <Input
                id="proj-branch"
                value={defaultBranch}
                onChange={(e) => setDefaultBranch(e.target.value)}
              />
            </div>
            <div className="space-y-2">
              <Label>Color</Label>
              <div className="flex gap-2">
                {SUGGESTED_COLORS.map((c) => (
                  <button
                    key={c}
                    type="button"
                    onClick={() => setColor(c)}
                    className={`h-6 w-6 rounded-full border-2 transition-colors ${
                      color === c ? "border-foreground" : "border-transparent"
                    }`}
                    style={{ background: c }}
                    aria-label={c}
                  />
                ))}
              </div>
            </div>
            <div className="space-y-2">
              <Label>Warm worktrees</Label>
              <p className="text-muted-foreground/80 text-xs">
                Pre-populate new worktrees with a preset's links (node_modules,
                .env, build caches). Skippable — change later on the repo's page.
              </p>
              <div className="bg-muted/40 flex flex-wrap gap-0.5 rounded-md p-0.5">
                <PresetOption
                  active={presetId === null}
                  onClick={() => setPresetId(null)}
                  label="Leave cold"
                />
                {presets.map((p) => (
                  <PresetOption
                    key={p.id}
                    active={presetId === p.id}
                    onClick={() => setPresetId(p.id)}
                    label={p.name}
                    title={p.paths.join(", ")}
                  />
                ))}
              </div>
            </div>
            {error && (
              <p className="text-destructive text-sm" role="alert">
                {error}
              </p>
            )}
          </div>
        )}

        <DialogFooter>
          {stage === "pick" && (
            <Button
              variant="ghost"
              onClick={() => requestClose(false)}
              disabled={busy}
            >
              Cancel
            </Button>
          )}
          {stage === "confirm" && (
            <>
              <Button
                variant="ghost"
                onClick={() => setStage("pick")}
                disabled={busy}
              >
                Back
              </Button>
              <Button
                onClick={onConfirm}
                disabled={busy || name.trim().length === 0}
              >
                {busy ? "Adding…" : "Add repo"}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
