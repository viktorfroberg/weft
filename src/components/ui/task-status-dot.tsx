import type { TaskStatus } from "@/lib/commands";

/** Tailwind `bg-*` class per task status. Shared across sidebar rows,
 *  task list rows, and anywhere else we show a status dot — keeps colors
 *  consistent (e.g. `waiting` is always amber, not sometimes yellow). */
export const TASK_STATUS_BG: Record<TaskStatus, string> = {
  idle: "bg-muted-foreground/40",
  working: "bg-emerald-400",
  waiting: "bg-amber-400",
  error: "bg-red-400",
  done: "bg-muted-foreground/30",
};

interface Props {
  status: TaskStatus;
  /** Size tier. `xs` (6px) for dense sidebar nesting, `sm` (8px) for
   *  list rows, `md` (10px) for headers. */
  size?: "xs" | "sm" | "md";
  /** Pulse while `status === "working"`? Subtle attention hint that the
   *  task is actively doing something. Opt-in because it's easy to
   *  over-use in lists. */
  pulse?: boolean;
  className?: string;
}

export function TaskStatusDot({
  status,
  size = "sm",
  pulse = false,
  className = "",
}: Props) {
  const dim = size === "xs" ? "h-1.5 w-1.5" : size === "sm" ? "h-2 w-2" : "h-2.5 w-2.5";
  const bg = TASK_STATUS_BG[status];
  const pulsing = pulse && status === "working" ? "animate-pulse" : "";
  return (
    <span
      className={`inline-block shrink-0 rounded-full ${dim} ${bg} ${pulsing} ${className}`}
      title={status}
      aria-label={`status: ${status}`}
    />
  );
}
