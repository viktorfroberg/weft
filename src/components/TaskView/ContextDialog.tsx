import { useEffect, useMemo, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Loader2, RefreshCw } from "lucide-react";
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
import { Textarea } from "@/components/ui/textarea";
import {
  taskContextGet,
  taskContextSet,
  taskRefreshTicketTitles,
} from "@/lib/commands";

interface Props {
  taskId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const AUTO_BEGIN =
  "<!-- weft:auto-begin — regenerated on task changes, do not hand-edit -->";
const AUTO_END = "<!-- weft:auto-end -->";
const NOTES_BEGIN =
  "<!-- weft:notes-begin — free-form scratch space; user AND agents may edit; preserved across regeneration -->";
const NOTES_END = "<!-- weft:notes-end -->";

/** Split the on-disk context.md into its auto + notes bodies. Mirrors
 *  the Rust splicer, but forgivingly: if anything is malformed we just
 *  show the raw file as-is in the auto preview and leave the notes
 *  editor empty. The Rust side will quarantine on its next write. */
function splitSections(raw: string): { auto: string; notes: string } {
  if (!raw.trim()) return { auto: "", notes: "" };
  const trimmed = raw.startsWith("\uFEFF") ? raw.slice(1) : raw;
  const ab = trimmed.indexOf(AUTO_BEGIN);
  const ae = trimmed.indexOf(AUTO_END);
  const nb = trimmed.indexOf(NOTES_BEGIN);
  const ne = trimmed.indexOf(NOTES_END);
  if (ab === -1 && ae === -1 && nb === -1 && ne === -1) {
    return { auto: "", notes: trimmed };
  }
  if (ab !== -1 && ae !== -1 && nb !== -1 && ne !== -1 && ab < ae && ae < nb && nb < ne) {
    const auto = trimmed.slice(ab + AUTO_BEGIN.length, ae).replace(/^\n+|\n+$/g, "");
    const notes = trimmed
      .slice(nb + NOTES_BEGIN.length, ne)
      .replace(/^\n+|\n+$/g, "");
    return { auto, notes };
  }
  return { auto: trimmed, notes: "" };
}

/**
 * Editor for the shared task context. Two stacked sections:
 *
 *  1. **Auto preview** — the `weft:auto` block rendered monospace
 *     read-only. Live-refreshes whenever the task changes (db_event
 *     bridge invalidates the query) so you see new ticket titles /
 *     added repos without closing the dialog.
 *
 *  2. **Notes editor** — the `weft:notes` block, editable by user
 *     AND by any agent in the task. Dirty-tracked: if the task
 *     mutates while the user is editing, the auto preview refreshes
 *     but the notes editor keeps your unsaved text and shows a
 *     banner so you know the file changed underneath. Saving
 *     re-splices your notes into the (possibly updated) auto block.
 *
 *  A "Refresh ticket titles" button re-fetches cached metadata for
 *  every linked ticket from its provider and rewrites the sidecar,
 *  so the auto preview shows current titles even after upstream
 *  rename.
 */
export function ContextDialog({ taskId, open, onOpenChange }: Props) {
  const qc = useQueryClient();
  const [raw, setRaw] = useState("");
  const [notesDraft, setNotesDraft] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [staleBanner, setStaleBanner] = useState(false);
  const savedNotesRef = useRef("");

  const load = async () => {
    setLoading(true);
    try {
      const content = await taskContextGet(taskId);
      setRaw(content);
      const { notes } = splitSections(content);
      savedNotesRef.current = notes;
      // Only clobber the editor if the user hasn't started typing yet
      // — the "dirty" preservation rule.
      setNotesDraft((prev) => {
        if (prev === "" || prev === savedNotesRef.current) {
          return notes;
        }
        return prev;
      });
    } catch {
      setRaw("");
      savedNotesRef.current = "";
      setNotesDraft("");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (!open) {
      // Reset on close so next open starts fresh.
      setRaw("");
      setNotesDraft("");
      savedNotesRef.current = "";
      setStaleBanner(false);
      return;
    }
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, taskId]);

  // Live refresh: any task mutation (ticket link, repo add, title
  // refresh) invalidates `qk.tasksAll()` via the db-event bridge. We
  // subscribe so the auto block stays current. Notes-dirty guard:
  // if the user has unsaved edits we only refresh the auto preview
  // and flash a banner; the editor keeps their draft.
  useEffect(() => {
    if (!open) return;
    const unsub = qc.getQueryCache().subscribe((event) => {
      const key = event.query.queryKey;
      if (!Array.isArray(key)) return;
      const first = key[0];
      if (first !== "tasks" && first !== "taskTickets" && first !== "taskWorktrees") {
        return;
      }
      // Any relevant invalidate triggers a reload.
      const dirty = notesDraft !== savedNotesRef.current;
      void (async () => {
        try {
          const content = await taskContextGet(taskId);
          setRaw(content);
          const { notes } = splitSections(content);
          if (dirty && notes !== savedNotesRef.current) {
            setStaleBanner(true);
          } else {
            savedNotesRef.current = notes;
            if (!dirty) setNotesDraft(notes);
          }
        } catch {
          // Ignore — fs hiccup shouldn't collapse the dialog.
        }
      })();
    });
    return () => unsub();
  }, [open, qc, taskId, notesDraft]);

  const { auto } = useMemo(() => splitSections(raw), [raw]);

  const isDirty = notesDraft !== savedNotesRef.current;

  const onSave = async () => {
    setSaving(true);
    try {
      await taskContextSet(taskId, notesDraft);
      savedNotesRef.current = notesDraft;
      setStaleBanner(false);
      toast.success(
        notesDraft.trim() ? "Notes saved across worktrees" : "Notes cleared",
      );
      onOpenChange(false);
    } catch (e) {
      toast.error("Couldn't save notes", { description: String(e) });
    } finally {
      setSaving(false);
    }
  };

  const onDiscard = () => {
    // Pull the latest notes from disk over the dirty draft.
    void load();
    setStaleBanner(false);
  };

  const onRefreshTitles = async () => {
    setRefreshing(true);
    try {
      const n = await taskRefreshTicketTitles(taskId);
      toast.success(
        n > 0 ? `Refreshed ${n} ticket${n === 1 ? "" : "s"}` : "No changes",
      );
      // task_refresh_ticket_titles emits a db_event, which will flow
      // through the subscription above and reload our auto preview.
    } catch (e) {
      toast.error("Couldn't refresh tickets", { description: String(e) });
    } finally {
      setRefreshing(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Task context</DialogTitle>
          <DialogDescription>
            Shared brief written to{" "}
            <code className="font-mono text-xs">.weft/context.md</code> in every
            worktree and mirrored as{" "}
            <code className="font-mono text-xs">CLAUDE.md</code> at the task
            root. Agents joining a second tab read this automatically.
          </DialogDescription>
        </DialogHeader>

        {loading ? (
          <div className="text-muted-foreground flex items-center gap-2 py-6 text-sm">
            <Loader2 size={14} className="animate-spin" />
            loading…
          </div>
        ) : (
          <div className="flex flex-col gap-4">
            <section className="flex flex-col gap-2">
              <div className="flex items-center justify-between">
                <h4 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
                  Auto — user intent, tickets, repos
                </h4>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={onRefreshTitles}
                  disabled={refreshing}
                  className="h-6 gap-1 text-xs"
                  title="Re-fetch ticket titles from Linear"
                >
                  {refreshing ? (
                    <Loader2 size={10} className="animate-spin" />
                  ) : (
                    <RefreshCw size={10} />
                  )}
                  Refresh tickets
                </Button>
              </div>
              <pre className="bg-muted/40 max-h-60 overflow-y-auto overflow-x-hidden whitespace-pre-wrap break-words rounded border px-3 py-2 font-mono text-xs">
                {auto.trim() || "(empty — a task with no prompt or tickets shows nothing here)"}
              </pre>
            </section>

            <section className="flex flex-col gap-2">
              <div className="flex items-center justify-between">
                <h4 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
                  Notes — shared scratch space
                </h4>
                {isDirty && (
                  <span className="text-xs text-muted-foreground">
                    unsaved
                  </span>
                )}
              </div>
              {staleBanner && (
                <div className="flex items-center justify-between rounded border border-amber-500/30 bg-amber-500/10 px-2 py-1 text-xs">
                  <span>Task context updated. Save or discard to reload.</span>
                  <button
                    type="button"
                    onClick={onDiscard}
                    className="underline underline-offset-2 hover:text-foreground"
                  >
                    Discard &amp; reload
                  </button>
                </div>
              )}
              <Textarea
                value={notesDraft}
                onChange={(e) => setNotesDraft(e.target.value)}
                placeholder="Ideas, gotchas, checkpoint notes. Agents can read and write here too."
                className="min-h-[160px] resize-y font-mono text-xs"
              />
            </section>
          </div>
        )}

        <DialogFooter>
          <Button
            variant="ghost"
            onClick={() => onOpenChange(false)}
            disabled={saving}
          >
            Close
          </Button>
          <Button
            onClick={onSave}
            disabled={saving || loading || !isDirty}
            className="gap-1"
          >
            {saving && <Loader2 size={12} className="animate-spin" />}
            Save notes
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
