import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { taskRename, type Task } from "@/lib/commands";

interface Props {
  task: Task;
  /** Tailwind class for the non-editing display. Caller controls
   *  size/weight/alignment so the same rename affordance can live in a
   *  breadcrumb, a page title, or a list row. */
  className?: string;
  /** When true (default), render the underlying text selectable so the
   *  user's first double-click selects the word before our dblclick
   *  kicks in. Disable for table cells where selection would be
   *  distracting. */
  selectable?: boolean;
}

/** Double-click to rename a task inline. Enter/blur commits, Esc
 *  cancels. Persists via `task_rename` which sets
 *  `tasks.name_locked_at` so the background `claude -p` auto-rename
 *  skips this row going forward. Matches the rename-in-place pattern
 *  ChatGPT / Cursor eventually ship for their chat sidebar items.
 *
 *  Extracted from the Toolbar in April 2026 when the task title moved
 *  from the window breadcrumb into the TaskView header. Kept
 *  controllable via `className` so future sites (sidebar context menu,
 *  recent-tasks switcher) can reuse the same interaction without
 *  copy-paste. */
export function InlineTaskRename({ task, className, selectable = true }: Props) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(task.name);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const taskIdRef = useRef(task.id);
  // Which task.id the current edit session began on. Checked at commit
  // time so a blur fired *after* the prop swapped to a different task
  // (TaskView is kept mounted across route changes by the panel pool)
  // doesn't write the old task's draft onto the new task.
  const editStartedForRef = useRef<string | null>(null);

  useEffect(() => {
    const idChanged = taskIdRef.current !== task.id;
    taskIdRef.current = task.id;
    if (idChanged) {
      // Task identity swap while this instance was reused. Drop any
      // in-flight edit so the pending blur→commit doesn't fire against
      // the new task, and resync the display to the new name.
      editStartedForRef.current = null;
      setEditing(false);
      setDraft(task.name);
    } else if (!editing) {
      // Same task, upstream rename landed (background LLM, another
      // window) — refresh the non-editing display.
      setDraft(task.name);
    }
  }, [task.id, task.name, editing]);

  useEffect(() => {
    if (editing) {
      requestAnimationFrame(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
      });
    }
  }, [editing]);

  const commit = async () => {
    const startedFor = editStartedForRef.current;
    editStartedForRef.current = null;
    setEditing(false);
    // Blur landing after a task swap — discard.
    if (!startedFor || startedFor !== task.id) {
      setDraft(task.name);
      return;
    }
    const next = draft.trim();
    if (!next || next === task.name) {
      setDraft(task.name);
      return;
    }
    try {
      await taskRename(task.id, next);
    } catch (e) {
      toast.error("Couldn't rename task", { description: String(e) });
      setDraft(task.name);
    }
  };

  if (!editing) {
    return (
      <span
        data-tauri-drag-region="false"
        onDoubleClick={() => {
          editStartedForRef.current = task.id;
          setDraft(task.name);
          setEditing(true);
        }}
        title="Double-click to rename"
        className={`${className ?? ""} ${selectable ? "cursor-text" : ""}`}
      >
        {task.name}
      </span>
    );
  }

  // Input mode strips `truncate` from the caller's className. On a
  // <span> truncate works as expected; on an <input> it collapses the
  // element toward its intrinsic size and clips the caret. Defending
  // here means callers can keep their read-state truncate without
  // worrying about it leaking into edit mode. `w-full` then lets the
  // input fill its flex slot.
  const inputClassName = `${(className ?? "")
    .replace(/\btruncate\b/g, "")
    .trim()} bg-transparent border-b border-foreground/30 focus:border-foreground/60 outline-none w-full`;

  return (
    <input
      ref={inputRef}
      data-tauri-drag-region="false"
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => void commit()}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          void commit();
        } else if (e.key === "Escape") {
          e.preventDefault();
          editStartedForRef.current = null;
          setDraft(task.name);
          setEditing(false);
        }
      }}
      className={inputClassName}
    />
  );
}
