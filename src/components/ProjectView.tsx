import { useState } from "react";
import {
  Check,
  Flame,
  FolderGit2,
  Plus,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react";
import { toast } from "sonner";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  projectDelete,
  projectLinksPresetApply,
  projectLinksReapply,
  projectLinksSet,
  projectLinksWarmUpMain,
  projectRename,
  projectSetColor,
  type LinkType,
  type Project,
  type ProjectLinkInput,
  type ProjectLinkRow,
} from "@/lib/commands";
import {
  useProjectLinkHealth,
  useProjectLinks,
  useProjectLinkPresets,
} from "@/stores/project_links";
import { useProjects } from "@/stores/projects";
import { useUi } from "@/stores/ui";
import { qk } from "@/query";
import { CHART_COLORS, hashedChartColor } from "@/lib/colors";
import { useConfirm } from "@/components/ConfirmDialog";
import { ProjectBadge } from "@/components/ProjectBadge";
import { useNavigateRoute } from "@/lib/active-route";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

interface Props {
  projectId: string;
}

/**
 * Dedicated per-repo settings page at `/projects/:projectId`. Promoted
 * out of the Settings tab so each registered git repo is a first-class
 * entity with its own route, mirroring how tasks work.
 *
 * Contains three stacked sub-sections:
 *   1. Identity — rename, color, delete.
 *   2. Warm worktrees — preset picker + per-path link editor.
 *   3. Maintenance — warm-up-main-checkout + re-apply to worktrees.
 *
 * Toolbar breadcrumb owns the "weft / {name}" heading — no inner header
 * here, matching TaskView's pattern.
 */
export function ProjectView({ projectId }: Props) {
  const { data: projects = [] } = useProjects();
  const navigate = useNavigateRoute();
  const setAddProjectOpen = useUi((s) => s.setAddProjectOpen);
  const project = projects.find((p) => p.id === projectId);

  return (
    <div className="grid h-full grid-cols-[200px_1fr] overflow-hidden">
      <nav className="border-border flex flex-col overflow-y-auto border-r">
        <div className="border-border flex items-center justify-between border-b px-3 py-2">
          <span className="text-muted-foreground text-[11px] font-medium uppercase tracking-wider">
            Repos
          </span>
          <button
            type="button"
            onClick={() => setAddProjectOpen(true)}
            className="hover:bg-accent text-muted-foreground hover:text-foreground flex h-5 w-5 items-center justify-center rounded transition-colors"
            title="Add a repo  ⌘P"
            aria-label="Add a repo"
          >
            <Plus size={12} />
          </button>
        </div>
        <div className="flex flex-1 flex-col gap-0.5 p-2">
          {projects.length === 0 ? (
            <button
              type="button"
              onClick={() => setAddProjectOpen(true)}
              className="hover:bg-accent text-muted-foreground hover:text-foreground flex items-center gap-2 rounded px-2 py-1.5 text-left text-xs transition-colors"
            >
              <Plus size={12} />
              Add a repo
            </button>
          ) : (
            projects.map((p) => {
              const active = p.id === projectId;
              return (
                <button
                  key={p.id}
                  type="button"
                  onClick={() => navigate({ kind: "project", id: p.id })}
                  className={`flex items-center gap-2 rounded px-2 py-1.5 text-left text-sm transition-colors ${
                    active
                      ? "bg-accent text-accent-foreground"
                      : "text-muted-foreground hover:bg-accent hover:text-foreground"
                  }`}
                  title={p.main_repo_path}
                >
                  <ProjectBadge name={p.name} color={p.color} size="sm" />
                  <span className="truncate">{p.name}</span>
                </button>
              );
            })
          )}
        </div>
      </nav>

      <div className="flex-1 overflow-y-auto p-6">
        {project ? (
          <div className="mx-auto max-w-2xl space-y-6">
            <Header project={project} />
            <SubSection label="Identity">
              <IdentityEditor project={project} />
            </SubSection>
            <SubSection
              label="Warm worktrees"
              description="Materialize heavy paths (node_modules, target/, .env) into every task's worktree on create. Symlinks for deps, APFS clones for build caches."
            >
              <WarmWorktreesEditor projectId={project.id} />
            </SubSection>
            <SubSection label="Maintenance">
              <MaintenanceControls projectId={project.id} />
            </SubSection>
          </div>
        ) : (
          <div className="mx-auto max-w-2xl">
            <p className="text-muted-foreground text-sm">
              {projects.length === 0
                ? "No repos registered yet. Add one from the left to get started."
                : "This repo no longer exists. Pick another from the left."}
            </p>
          </div>
        )}
      </div>
    </div>
  );
}

function Header({ project }: { project: Project }) {
  return (
    <header className="flex items-center gap-3">
      <ProjectBadge name={project.name} color={project.color} size="md" />
      <div className="min-w-0 flex-1">
        <h1 className="truncate text-lg font-semibold">{project.name}</h1>
        <p className="text-muted-foreground truncate font-mono text-xs">
          {project.main_repo_path}
        </p>
      </div>
      <span className="text-muted-foreground flex items-center gap-1 text-xs">
        <FolderGit2 size={12} />
        default: {project.default_branch}
      </span>
    </header>
  );
}

function SubSection({
  label,
  description,
  children,
}: {
  label: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <section className="border-border bg-card rounded-lg border p-4">
      <div className="mb-3">
        <h2 className="text-sm font-semibold">{label}</h2>
        {description && (
          <p className="text-muted-foreground mt-0.5 text-xs">{description}</p>
        )}
      </div>
      {children}
    </section>
  );
}

function IdentityEditor({ project }: { project: Project }) {
  const qc = useQueryClient();
  const confirm = useConfirm();
  const navigate = useNavigateRoute();
  const [name, setName] = useState(project.name);

  const saveName = useMutation({
    mutationFn: (next: string) => projectRename(project.id, next),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.projects() });
      toast.success("Renamed");
    },
    onError: (e) => toast.error("Rename failed", { description: String(e) }),
  });

  const setColor = useMutation({
    mutationFn: (color: string | null) => projectSetColor(project.id, color),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.projects() });
    },
    onError: (e) =>
      toast.error("Color change failed", { description: String(e) }),
  });

  const del = async () => {
    const ok = await confirm({
      title: `Delete "${project.name}"?`,
      description:
        "The git repository on disk is NOT touched. Active task worktrees continue to work. Future tasks will no longer be able to pick this repo.",
      confirmText: "Delete",
      destructive: true,
    });
    if (!ok) return;
    try {
      await projectDelete(project.id);
      toast.success(`Deleted “${project.name}”`);
      void qc.invalidateQueries({ queryKey: qk.projects() });
      // Current route is now stale — page will show empty state if we
      // stay here. Navigate home so the user lands somewhere meaningful.
      navigate({ kind: "home" });
    } catch (e) {
      toast.error("Delete failed", { description: String(e) });
    }
  };

  const commitName = () => {
    const trimmed = name.trim();
    if (!trimmed || trimmed === project.name) {
      setName(project.name);
      return;
    }
    saveName.mutate(trimmed);
  };

  const activeColor = project.color;
  const autoColor = hashedChartColor(project.name);

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <span className="text-muted-foreground w-12 shrink-0 text-[11px]">
          Name
        </span>
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={commitName}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              (e.target as HTMLInputElement).blur();
            }
            if (e.key === "Escape") {
              setName(project.name);
              (e.target as HTMLInputElement).blur();
            }
          }}
          className="h-7 flex-1 text-xs"
        />
      </div>
      <div className="flex items-center gap-2">
        <span className="text-muted-foreground w-12 shrink-0 text-[11px]">
          Color
        </span>
        <div className="flex flex-wrap items-center gap-1.5">
          {CHART_COLORS.map((c) => {
            const selected = activeColor === c;
            return (
              <button
                key={c}
                type="button"
                onClick={() => setColor.mutate(c)}
                className={`relative h-5 w-5 rounded-full border transition-all ${
                  selected
                    ? "border-foreground ring-foreground/30 ring-2"
                    : "border-border hover:scale-110"
                }`}
                style={{ background: c }}
                title={selected ? "Selected" : "Pick color"}
              >
                {selected && (
                  <Check
                    size={11}
                    className="absolute inset-0 m-auto text-white mix-blend-difference"
                  />
                )}
              </button>
            );
          })}
          <button
            type="button"
            onClick={() => setColor.mutate(null)}
            className={`flex h-5 items-center rounded-full border px-2 text-[10px] transition-colors ${
              activeColor === null
                ? "border-foreground bg-accent"
                : "border-border hover:bg-accent"
            }`}
            title={`Reset to auto (${autoColor})`}
          >
            Auto
          </button>
        </div>
      </div>
      <div className="flex justify-end">
        <Button
          size="sm"
          variant="destructive"
          onClick={del}
          className="h-7 gap-1 text-xs"
        >
          <Trash2 size={11} />
          Delete repo
        </Button>
      </div>
    </div>
  );
}

function WarmWorktreesEditor({ projectId }: { projectId: string }) {
  const { data: links = [] } = useProjectLinks(projectId);
  return (
    <div className="space-y-3">
      <PresetControls projectId={projectId} currentCount={links.length} />
      <LinkEditor projectId={projectId} initialLinks={links} />
      <HealthLine projectId={projectId} linkCount={links.length} />
    </div>
  );
}

function HealthLine({
  projectId,
  linkCount,
}: {
  projectId: string;
  linkCount: number;
}) {
  const { data: health } = useProjectLinkHealth(projectId);
  if (linkCount === 0) {
    return (
      <p className="text-muted-foreground text-[11px]">
        No links — tasks on this project start cold.
      </p>
    );
  }
  if (!health || health.summary.total === 0) {
    return (
      <p className="text-muted-foreground text-[11px]">
        {linkCount} link{linkCount === 1 ? "" : "s"} configured. No active
        worktrees yet.
      </p>
    );
  }
  const { ok, missing, dangling, mismatched, total } = health.summary;
  const bad = dangling + missing;
  let color = "bg-emerald-500";
  let msg = `${ok}/${total} links ok across active worktrees`;
  if (bad > 0) {
    color = "bg-red-500";
    msg = `${bad} dangling / missing — run Re-apply to fix`;
  } else if (mismatched > 0) {
    color = "bg-amber-500";
    msg = `${mismatched} mismatched (unexpected on-disk shape)`;
  }
  return (
    <p className="text-muted-foreground flex items-center gap-1.5 text-[11px]">
      <span className={`inline-block h-2 w-2 rounded-full ${color}`} />
      {msg}
    </p>
  );
}

function MaintenanceControls({ projectId }: { projectId: string }) {
  const { data: links = [] } = useProjectLinks(projectId);
  const hasLinks = links.length > 0;

  const reapply = useMutation({
    mutationFn: () => projectLinksReapply(projectId),
    onSuccess: (r) => {
      if (r.worktrees_failed.length === 0) {
        toast.success(
          `Re-applied to ${r.worktrees_touched} worktree${r.worktrees_touched === 1 ? "" : "s"}`,
        );
      } else {
        toast.warning(
          `Re-applied to ${r.worktrees_touched} worktree${r.worktrees_touched === 1 ? "" : "s"}, ${r.worktrees_failed.length} failed`,
          { description: r.worktrees_failed.join("\n") },
        );
      }
    },
    onError: (e) => toast.error("Re-apply failed", { description: String(e) }),
  });

  const warmUp = useMutation({
    mutationFn: () => projectLinksWarmUpMain(projectId),
    onSuccess: (r) => {
      if (r.success) {
        toast.success(`Ran \`${r.command}\` in main checkout`, {
          description: r.stdout.trim().split("\n").slice(-1)[0] || undefined,
        });
      } else {
        toast.error(`\`${r.command}\` failed`, {
          description: r.stderr.trim().split("\n").slice(-3).join("\n"),
        });
      }
    },
    onError: (e) => toast.error("Warm-up failed", { description: String(e) }),
  });

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <Button
        size="sm"
        variant="ghost"
        onClick={() => warmUp.mutate()}
        disabled={warmUp.isPending}
        className="h-7 gap-1 text-xs"
        title="Run the project's install command (bun/npm/pnpm/yarn/cargo/pip) in the main checkout, so future tasks can symlink into a populated dependency tree"
      >
        <Flame size={12} />
        {warmUp.isPending ? "Warming up…" : "Warm up main checkout"}
      </Button>
      {hasLinks && (
        <Button
          size="sm"
          variant="ghost"
          onClick={() => reapply.mutate()}
          disabled={reapply.isPending}
          className="h-7 gap-1 text-xs"
          title="Re-materialize this project's links into every active worktree. Safe no-op when nothing changed."
        >
          <RefreshCw size={12} />
          {reapply.isPending ? "Re-applying…" : "Re-apply to worktrees"}
        </Button>
      )}
    </div>
  );
}

function PresetControls({
  projectId,
  currentCount,
}: {
  projectId: string;
  currentCount: number;
}) {
  const { data: presets = [] } = useProjectLinkPresets();

  const applyPreset = useMutation({
    mutationFn: (presetId: string) =>
      projectLinksPresetApply(projectId, presetId),
    onSuccess: (_, presetId) => {
      toast.success(
        `Applied “${presets.find((p) => p.id === presetId)?.name ?? presetId}” preset`,
      );
    },
    onError: (e) =>
      toast.error("Couldn't apply preset", { description: String(e) }),
  });

  const clearAll = useMutation({
    mutationFn: () => projectLinksSet(projectId, []),
    onSuccess: () => toast.success("Cleared all links"),
    onError: (e) => toast.error("Clear failed", { description: String(e) }),
  });

  return (
    <div>
      <p className="text-muted-foreground mb-1.5 text-[11px]">
        Apply a preset (replaces current links)
      </p>
      <div className="flex flex-wrap gap-1">
        {presets.map((p) => (
          <Button
            key={p.id}
            size="sm"
            variant="ghost"
            onClick={() => applyPreset.mutate(p.id)}
            disabled={applyPreset.isPending}
            className="h-7 text-xs"
            title={p.paths.join(", ")}
          >
            {p.name}
          </Button>
        ))}
        {currentCount > 0 && (
          <Button
            size="sm"
            variant="ghost"
            onClick={() => clearAll.mutate()}
            disabled={clearAll.isPending}
            className="text-muted-foreground hover:text-destructive ml-auto h-7 text-xs"
          >
            <Trash2 size={11} className="mr-1" />
            Clear
          </Button>
        )}
      </div>
    </div>
  );
}

function LinkEditor({
  projectId,
  initialLinks,
}: {
  projectId: string;
  initialLinks: ProjectLinkRow[];
}) {
  const [draft, setDraft] = useState<ProjectLinkInput[]>(() =>
    initialLinks.map((l) => ({ path: l.path, link_type: l.link_type })),
  );
  const [synced, setSynced] = useState<string>(
    initialLinks.map((l) => `${l.path}:${l.link_type}`).join(","),
  );
  const remoteKey = initialLinks
    .map((l) => `${l.path}:${l.link_type}`)
    .join(",");
  if (remoteKey !== synced) {
    setDraft(
      initialLinks.map((l) => ({ path: l.path, link_type: l.link_type })),
    );
    setSynced(remoteKey);
  }

  const [newPath, setNewPath] = useState("");
  const [newType, setNewType] = useState<LinkType>("symlink");
  const qc = useQueryClient();

  const save = useMutation({
    mutationFn: () => projectLinksSet(projectId, draft),
    onSuccess: () => {
      toast.success("Links saved");
      void qc.invalidateQueries({ queryKey: qk.projectLinks(projectId) });
    },
    onError: (e) => toast.error("Save failed", { description: String(e) }),
  });

  const addOne = () => {
    const p = newPath.trim();
    if (!p) return;
    if (draft.some((d) => d.path === p)) {
      toast.warning(`${p} is already linked`);
      return;
    }
    setDraft((prev) => [...prev, { path: p, link_type: newType }]);
    setNewPath("");
  };

  const remove = (path: string) =>
    setDraft((prev) => prev.filter((d) => d.path !== path));

  const toggleType = (path: string) =>
    setDraft((prev) =>
      prev.map((d) =>
        d.path === path
          ? {
              ...d,
              link_type: d.link_type === "symlink" ? "clone" : "symlink",
            }
          : d,
      ),
    );

  const dirty =
    draft.map((l) => `${l.path}:${l.link_type}`).join(",") !== remoteKey;

  return (
    <div className="space-y-2">
      <p className="text-muted-foreground text-[11px]">Paths (repo-relative)</p>
      {draft.length === 0 ? (
        <p className="text-muted-foreground/70 text-xs italic">
          No links — tasks on this project start cold.
        </p>
      ) : (
        <ul className="space-y-1">
          {draft.map((l) => (
            <li
              key={l.path}
              className="border-border flex items-center gap-2 rounded border px-2 py-1 text-xs"
            >
              <code className="text-foreground flex-1 truncate font-mono">
                {l.path}
              </code>
              <button
                type="button"
                onClick={() => toggleType(l.path)}
                className="text-muted-foreground hover:text-foreground bg-muted rounded px-1.5 py-0.5 font-mono text-[10px]"
                title="Toggle symlink ↔ clone"
              >
                {l.link_type}
              </button>
              <button
                type="button"
                onClick={() => remove(l.path)}
                className="text-muted-foreground hover:text-destructive"
                title="Remove"
              >
                <X size={12} />
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="flex items-center gap-1.5">
        <Input
          value={newPath}
          onChange={(e) => setNewPath(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              addOne();
            }
          }}
          placeholder="e.g. node_modules or .env.local"
          className="h-7 font-mono text-xs"
        />
        <select
          value={newType}
          onChange={(e) => setNewType(e.target.value as LinkType)}
          className="bg-background border-border h-7 rounded border px-1.5 text-xs"
          title="symlink for deps/env, clone for build caches"
        >
          <option value="symlink">symlink</option>
          <option value="clone">clone</option>
        </select>
        <Button
          size="sm"
          variant="ghost"
          onClick={addOne}
          disabled={!newPath.trim()}
          className="h-7 text-xs"
        >
          <Plus size={12} />
        </Button>
      </div>
      {dirty && (
        <div className="flex justify-end">
          <Button
            size="sm"
            onClick={() => save.mutate()}
            disabled={save.isPending}
            className="h-7 text-xs"
          >
            {save.isPending ? "Saving…" : "Save"}
          </Button>
        </div>
      )}
    </div>
  );
}
