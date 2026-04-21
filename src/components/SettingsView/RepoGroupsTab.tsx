import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Check, Edit3, FolderGit2, Plus, Trash2, X } from "lucide-react";
import {
  workspaceAddRepo,
  workspaceCreate,
  workspaceDelete,
  workspaceRemoveRepo,
  type Project,
  type Workspace,
} from "@/lib/commands";
import { useWorkspaces, useWorkspaceRepos } from "@/stores/workspaces";
import { useProjects } from "@/stores/projects";
import { useConfirm } from "@/components/ConfirmDialog";
import { qk } from "@/query";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ProjectBadge } from "@/components/ProjectBadge";
import { Card } from "./Card";

/**
 * v1.0.7: "Repo groups" replaces the workspace-view page as the place
 * to manage saved repo bundles. Each group is a `workspaces` row with
 * its `workspace_repos` children. Creating a task with a group picks
 * its repo set + base-branch overrides by default (editable per-task).
 */
export function RepoGroupsTab() {
  const { data: groups = [] } = useWorkspaces();
  const [createOpen, setCreateOpen] = useState(false);

  return (
    <Card
      title="Repo groups"
      description="Saved sets of repos for multi-repo tasks. Pick one from the task-compose card to pre-fill repo selection + base-branch overrides. Not a navigation concept — purely a shortcut."
      Icon={FolderGit2}
    >
      {groups.length === 0 ? (
        <p className="text-muted-foreground text-xs">
          No repo groups yet. Save one from the task-compose card by
          filling in "Repo group name" before submitting.
        </p>
      ) : (
        <ul className="space-y-2">
          {groups.map((g) => (
            <GroupRow key={g.id} group={g} />
          ))}
        </ul>
      )}
      <div className="mt-3 flex justify-end">
        <Button
          size="sm"
          variant="ghost"
          onClick={() => setCreateOpen(true)}
          className="h-7 gap-1 text-xs"
        >
          <Plus size={12} />
          New repo group
        </Button>
      </div>
      {createOpen && (
        <CreateGroupInline onClose={() => setCreateOpen(false)} />
      )}
    </Card>
  );
}

function GroupRow({ group }: { group: Workspace }) {
  const { data: repos = [] } = useWorkspaceRepos(group.id);
  const { data: projects = [] } = useProjects();
  const [expanded, setExpanded] = useState(false);
  const confirm = useConfirm();
  const projectById = new Map(projects.map((p) => [p.id, p] as const));

  const onDelete = async () => {
    const ok = await confirm({
      title: `Delete repo group "${group.name}"?`,
      description:
        "Tasks previously created with this group are NOT affected (they keep their worktrees). The group's repo list is removed.",
      confirmText: "Delete",
      destructive: true,
    });
    if (!ok) return;
    try {
      await workspaceDelete(group.id);
      toast.success(`Deleted “${group.name}”`);
    } catch (e) {
      toast.error("Delete failed", { description: String(e) });
    }
  };

  return (
    <li className="border-border bg-card rounded-md border">
      <div className="flex items-center gap-2 px-3 py-2">
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="hover:bg-accent flex flex-1 items-center gap-2 rounded py-0.5 text-left text-sm"
        >
          <span className="text-muted-foreground text-xs">
            {expanded ? "▾" : "▸"}
          </span>
          <span className="font-medium">{group.name}</span>
          <span className="text-muted-foreground font-mono text-[10px]">
            {repos.length} repo{repos.length === 1 ? "" : "s"}
          </span>
          <span className="flex gap-0.5">
            {repos.slice(0, 5).map((r) => {
              const p = projectById.get(r.project_id);
              return (
                <span
                  key={r.project_id}
                  className="inline-block h-1.5 w-1.5 rounded-full"
                  style={{ background: p?.color ?? "var(--muted-foreground)" }}
                  title={p?.name}
                />
              );
            })}
          </span>
        </button>
        <Button
          size="sm"
          variant="ghost"
          onClick={onDelete}
          className="text-muted-foreground hover:text-destructive h-7 w-7 p-0"
          title="Delete group"
        >
          <Trash2 size={12} />
        </Button>
      </div>
      {expanded && (
        <GroupEditor group={group} repos={repos} projects={projects} />
      )}
    </li>
  );
}

function GroupEditor({
  group,
  repos,
  projects,
}: {
  group: Workspace;
  repos: ReturnType<typeof useWorkspaceRepos>["data"] extends infer U | undefined
    ? Exclude<U, undefined>
    : never;
  projects: Project[];
}) {
  const projectById = new Map(projects.map((p) => [p.id, p] as const));
  const attachedIds = new Set(repos?.map((r) => r.project_id) ?? []);
  const detached = projects.filter((p) => !attachedIds.has(p.id));
  const qc = useQueryClient();

  const add = useMutation({
    mutationFn: (projectId: string) =>
      workspaceAddRepo({
        workspace_id: group.id,
        project_id: projectId,
        base_branch: null,
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.workspaceRepos(group.id) });
    },
  });
  const remove = useMutation({
    mutationFn: (projectId: string) => workspaceRemoveRepo(group.id, projectId),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.workspaceRepos(group.id) });
    },
  });

  return (
    <div className="border-border border-t px-3 py-3">
      <p className="text-muted-foreground mb-1.5 text-[11px]">Attached repos</p>
      {repos && repos.length === 0 ? (
        <p className="text-muted-foreground/70 text-xs italic">
          No repos attached yet.
        </p>
      ) : (
        <ul className="space-y-1">
          {repos?.map((r) => {
            const p = projectById.get(r.project_id);
            return (
              <li
                key={r.project_id}
                className="border-border flex items-center gap-2 rounded border px-2 py-1 text-xs"
              >
                <ProjectBadge
                  name={p?.name ?? r.project_id}
                  color={p?.color}
                  size="sm"
                />
                <span className="flex-1 truncate">{p?.name ?? r.project_id}</span>
                <span className="text-muted-foreground font-mono text-[10px]">
                  {r.base_branch ?? p?.default_branch ?? "—"}
                </span>
                <button
                  type="button"
                  onClick={() => remove.mutate(r.project_id)}
                  disabled={remove.isPending}
                  className="text-muted-foreground hover:text-destructive"
                  title="Remove"
                >
                  <X size={12} />
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {detached.length > 0 && (
        <>
          <p className="text-muted-foreground mb-1.5 mt-3 text-[11px]">
            Add a repo
          </p>
          <div className="flex flex-wrap gap-1">
            {detached.map((p) => (
              <Button
                key={p.id}
                size="sm"
                variant="ghost"
                onClick={() => add.mutate(p.id)}
                disabled={add.isPending}
                className="h-7 gap-1 text-xs"
              >
                <ProjectBadge name={p.name} color={p.color} size="sm" />
                {p.name}
              </Button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

function CreateGroupInline({ onClose }: { onClose: () => void }) {
  const [name, setName] = useState("");
  const qc = useQueryClient();

  const create = useMutation({
    mutationFn: () => workspaceCreate({ name: name.trim() }),
    onSuccess: () => {
      toast.success(`Created “${name.trim()}”`);
      void qc.invalidateQueries({ queryKey: qk.workspaces() });
      onClose();
    },
    onError: (e) => toast.error("Create failed", { description: String(e) }),
  });

  return (
    <div className="border-border mt-2 flex items-center gap-2 rounded-md border p-2">
      <Edit3 size={12} className="text-muted-foreground" />
      <Input
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && name.trim()) {
            e.preventDefault();
            create.mutate();
          }
          if (e.key === "Escape") onClose();
        }}
        autoFocus
        placeholder="Group name"
        className="h-7 flex-1 text-xs"
      />
      <Button
        size="sm"
        onClick={() => create.mutate()}
        disabled={!name.trim() || create.isPending}
        className="h-7 w-7 p-0"
      >
        <Check size={12} />
      </Button>
      <Button
        size="sm"
        variant="ghost"
        onClick={onClose}
        className="h-7 w-7 p-0"
      >
        <X size={12} />
      </Button>
    </div>
  );
}
