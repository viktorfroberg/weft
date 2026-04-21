import type { LucideIcon } from "lucide-react";

/**
 * Settings section wrapper. Adds a bordered card with an icon chip
 * header so each section reads as a distinct unit instead of a wall of
 * form rows. Used by every Settings tab — keep the chrome consistent
 * across tabs, not the content.
 */
export function Card({
  title,
  description,
  Icon,
  children,
}: {
  title: string;
  description?: string;
  Icon?: LucideIcon;
  children: React.ReactNode;
}) {
  return (
    <section className="border-border bg-card rounded-lg border p-5">
      <header className="mb-4 flex items-start gap-3">
        {Icon && (
          <span className="bg-muted text-foreground mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-md">
            <Icon size={14} />
          </span>
        )}
        <div className="min-w-0 flex-1">
          <h2 className="text-sm font-semibold">{title}</h2>
          {description && (
            <p className="text-muted-foreground mt-0.5 text-xs">{description}</p>
          )}
        </div>
      </header>
      <div>{children}</div>
    </section>
  );
}
