import {
  hashedChartColor,
  readableText,
  tintedBackground,
  tintedBorder,
} from "@/lib/colors";
import { useEffectiveTheme } from "@/lib/theme";

interface Props {
  /** Used for the letter + hash-derived color fallback. */
  name: string;
  /** Explicit color — used for Projects (which have a picked color in DB). */
  color?: string | null;
  /** Size preset. `sm` = 16px for dense rows, `md` = 22px default sidebar. */
  size?: "sm" | "md";
  className?: string;
}

/**
 * Colored letter thumbnail. The first non-whitespace character of `name`
 * in a rounded square, tinted by `color` (or hashed from name if absent).
 *
 * Used for Workspaces + Projects in the sidebar and for task identity in
 * lists. Matches Superset's repo-thumbnail pattern but uses our oklch
 * chart tokens so it stays on-theme in both light + dark.
 */
export function ProjectBadge({
  name,
  color,
  size = "md",
  className = "",
}: Props) {
  const resolvedColor =
    color && color.length > 0 ? color : hashedChartColor(name);
  const appearance = useEffectiveTheme();
  const textColor = readableText(resolvedColor, appearance);
  const letter = (name.trim()[0] ?? "?").toUpperCase();

  const px = size === "sm" ? 16 : 22;
  const fontSize = size === "sm" ? 9 : 11;

  return (
    <span
      className={`inline-flex shrink-0 items-center justify-center rounded-[5px] border font-mono font-semibold ${className}`}
      style={{
        width: px,
        height: px,
        fontSize,
        background: tintedBackground(resolvedColor),
        borderColor: tintedBorder(resolvedColor),
        color: textColor,
      }}
      aria-hidden
    >
      {letter}
    </span>
  );
}
