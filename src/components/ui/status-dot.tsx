import { hashedChartColor } from "@/lib/colors";

interface Props {
  /** Explicit color (e.g. a project's picked color) OR a name to hash. */
  color?: string | null;
  /** If no `color`, hash this string to pick from the chart tokens. */
  name?: string;
  size?: "xs" | "sm" | "md";
  className?: string;
}

/**
 * One shared status-dot component for everywhere a repo is referenced at
 * low fidelity: sidebar rows, worktree pills, diff file-list. Was ad-hoc
 * across 4 files with different sizes — now one component, three sizes.
 */
export function StatusDot({
  color,
  name,
  size = "sm",
  className = "",
}: Props) {
  const resolved = color ?? (name ? hashedChartColor(name) : "var(--muted-foreground)");
  const px = size === "xs" ? 6 : size === "sm" ? 8 : 10;
  return (
    <span
      className={`inline-block shrink-0 rounded-full ${className}`}
      style={{
        width: px,
        height: px,
        background: resolved,
      }}
      aria-hidden
    />
  );
}
