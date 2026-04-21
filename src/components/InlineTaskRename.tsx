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

  useEffect(() => {
    // Whenever an upstream rename lands (background LLM, another
    // window, manual rename from the sidebar) refresh the non-editing
    // display so we don't strand a stale draft.
    if (!editing) setDraft(task.name);
  }, [task.name, editing]);

  useEffect(() => {
    if (editing) {
      requestAnimationFrame(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
      });
    }
  }, [editing]);

  const commit = async () => {
    const next = draft.trim();
    setEditing(false);
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
          setDraft(task.name);
          setEditing(false);
        }
      }}
      className={`${className ?? ""} bg-transparent border-b border-foreground/30 focus:border-foreground/60 outline-none min-w-0`}
    />
  );
}
