import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

interface Shortcut {
  keys: string;
  description: string;
  section: string;
}

const SHORTCUTS: Shortcut[] = [
  { keys: "⌘K", description: "Command palette (jump anywhere)", section: "Navigate" },
  { keys: "⌘⇧N", description: "New task (compose)", section: "Navigate" },
  { keys: "⌘⇧O", description: "Recent tasks", section: "Navigate" },
  { keys: "⌘P", description: "Add a repo", section: "Navigate" },
  { keys: "⌘1…⌘9", description: "Jump to task (or focus worktree in task view)", section: "Navigate" },
  { keys: "⌘B", description: "Toggle sidebar", section: "Navigate" },
  { keys: "⌘L", description: "Launch default agent (in task view)", section: "Task" },
  { keys: "⌘T", description: "New terminal tab picker (in task view)", section: "Task" },
  { keys: "⌘\\", description: "Toggle changes panel (in task view)", section: "Task" },
  { keys: "Esc", description: "Back / close", section: "Navigate" },
  { keys: "⌘/", description: "Show this overlay", section: "Help" },
  { keys: "⌘↵", description: "Commit all (in commit message)", section: "Changes" },
];

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function ShortcutsOverlay({ open, onOpenChange }: Props) {
  const sections = Array.from(new Set(SHORTCUTS.map((s) => s.section)));
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Keyboard shortcuts</DialogTitle>
        </DialogHeader>
        <div className="space-y-4">
          {sections.map((section) => (
            <div key={section}>
              <h3 className="text-muted-foreground mb-1.5 text-[10px] uppercase tracking-wider">
                {section}
              </h3>
              <ul className="space-y-1">
                {SHORTCUTS.filter((s) => s.section === section).map((s) => (
                  <li
                    key={s.keys}
                    className="flex items-center justify-between gap-4 text-sm"
                  >
                    <span>{s.description}</span>
                    <kbd className="bg-muted text-muted-foreground rounded border px-1.5 py-0.5 font-mono text-[11px]">
                      {s.keys}
                    </kbd>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
        <p className="text-muted-foreground/70 text-[11px]">
          User-configurable shortcuts coming in Settings.
        </p>
      </DialogContent>
    </Dialog>
  );
}
