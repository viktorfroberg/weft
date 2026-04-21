import { useMemo } from "react";
import { Check, ChevronRight } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { useProjects } from "@/stores/projects";
import { useIntegrations } from "@/stores/integrations";
import { usePrefs } from "@/stores/prefs";
import { useUi } from "@/stores/ui";
import { useNavigateRoute } from "@/lib/active-route";
import logoMark from "@/assets/logo-mark.png";

/**
 * First-run overlay. Only renders when `prefs.hasCompletedOnboarding` is
 * false AND there are zero projects (so upgrading users never see it).
 *
 * v1.0.7: workspaces are gone as a primary concept — tasks are ad-hoc
 * over any repo combo. Steps collapse to (1) add a repo, (2) optional
 * Linear. The old "Create a workspace" step is obsolete.
 */
export function Onboarding() {
  const { data: projects = [] } = useProjects();
  const { data: integrations = [] } = useIntegrations();
  const complete = usePrefs((s) => s.completeOnboarding);
  const userName = usePrefs((s) => s.userName);
  const setUserName = usePrefs((s) => s.setUserName);
  const autoRenameTasks = usePrefs((s) => s.autoRenameTasks);
  const setAutoRenameTasks = usePrefs((s) => s.setAutoRenameTasks);
  const setAddProjectOpen = useUi((s) => s.setAddProjectOpen);
  const navigate = useNavigateRoute();

  const hasProject = projects.length > 0;
  const hasLinear = integrations.some(
    (p) => p.id === "linear" && p.connected,
  );

  const step = useMemo(() => {
    if (!hasProject) return 1;
    return 2;
  }, [hasProject]);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm animate-in fade-in duration-150">
      <button
        type="button"
        onClick={complete}
        className="text-foreground hover:text-foreground absolute right-4 top-4 text-xs"
      >
        Skip intro
      </button>
      <div className="border-border bg-card text-card-foreground w-full max-w-md rounded-xl border p-6 shadow-2xl">
        <div className="mb-5 flex flex-col items-center gap-3">
          <img
            src={logoMark}
            alt=""
            className="h-16 w-16 select-none"
            draggable={false}
          />
          <h2 className="text-base font-semibold tracking-tight">
            Welcome to weft
          </h2>
        </div>

        <p className="text-foreground mb-5 text-sm leading-relaxed">
          Parallel coding-agent sessions across multiple repos. Register a
          repo, then start a task — agents get isolated worktrees per task.
        </p>

        {/* Display name — powers the Home greeting ("Good morning, Viktor").
            Stored locally in prefs; never sent anywhere. Editable later in
            Settings → Workflow. Onchange writes immediately so there's no
            save button to miss. */}
        <label className="mb-4 flex items-center gap-3 text-sm">
          <span className="text-foreground shrink-0">Call me</span>
          <Input
            value={userName}
            onChange={(e) => setUserName(e.target.value)}
            placeholder="Your first name"
            className="h-8 flex-1 text-sm"
            autoFocus
          />
        </label>

        {/* Auto-rename tasks — matches the ChatGPT pattern where your
            conversation gets a short label once it has some content.
            Uses `claude -p --model haiku`, which assumes Claude Code is
            installed. Toggled later in Settings → Workflow. */}
        <div className="mb-5 flex items-start justify-between gap-3 rounded-lg border border-border/60 bg-muted/20 p-3 text-sm">
          <div className="min-w-0">
            <div className="text-foreground font-medium">Auto-rename tasks</div>
            <p className="text-muted-foreground mt-0.5 text-xs leading-relaxed">
              Use your Claude Code install to turn long prompts into short
              sidebar labels. You can always rename a task manually.
            </p>
          </div>
          <Switch
            checked={autoRenameTasks}
            onCheckedChange={setAutoRenameTasks}
            className="mt-1 shrink-0"
          />
        </div>

        <ol className="space-y-2.5">
          <StepRow
            n={1}
            active={step === 1}
            done={hasProject}
            title="Add your first repo"
            subtitle="Register a git repository. weft creates isolated worktrees in it per task."
            ctaLabel={hasProject ? "Added" : "Add repo"}
            onCta={() => setAddProjectOpen(true)}
          />
          <StepRow
            n={2}
            active={step === 2}
            done={hasLinear}
            title="Connect Linear (optional)"
            subtitle="Link tickets at task-create; weft pulls title + status into the agent's first turn and keeps a shared brief at .weft/context.md."
            ctaLabel={hasLinear ? "Connected" : "Open settings"}
            onCta={() => navigate({ kind: "settings" })}
            disabled={!hasProject}
          />
        </ol>

        <div className="mt-5 flex items-center justify-between">
          <span className="text-foreground text-xs">
            {step === 2
              ? "All set — start from the compose card on Home."
              : `Step ${step} of 2`}
          </span>
          <Button
            size="sm"
            onClick={complete}
            className="h-7 gap-1 text-xs"
            variant={step === 2 ? "default" : "ghost"}
          >
            {step === 2 ? "Start working" : "Dismiss"}
            <ChevronRight size={12} />
          </Button>
        </div>
      </div>
    </div>
  );
}

interface StepRowProps {
  n: number;
  active: boolean;
  done: boolean;
  title: string;
  subtitle: string;
  ctaLabel: string;
  onCta: () => void;
  disabled?: boolean;
}

function StepRow({
  n,
  active,
  done,
  title,
  subtitle,
  ctaLabel,
  onCta,
  disabled,
}: StepRowProps) {
  return (
    <li
      className={`rounded-lg border p-3 transition-colors ${
        active ? "border-foreground/25 bg-accent/40" : "border-border"
      } ${done ? "opacity-60" : ""}`}
    >
      <div className="flex items-start gap-3">
        <span
          className={`mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-[11px] font-mono ${
            done
              ? "bg-emerald-500 text-white"
              : active
                ? "bg-foreground text-background"
                : "bg-muted text-foreground"
          }`}
        >
          {done ? <Check size={11} /> : n}
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex items-center justify-between gap-2">
            <span className="text-foreground text-sm font-medium">
              {title}
            </span>
            <Button
              size="sm"
              variant={active ? "outline" : "ghost"}
              onClick={onCta}
              disabled={disabled || done}
              className="h-6 shrink-0 px-2 text-[11px]"
            >
              {ctaLabel}
            </Button>
          </div>
          <p className="text-foreground mt-1 text-xs leading-relaxed">
            {subtitle}
          </p>
        </div>
      </div>
    </li>
  );
}
