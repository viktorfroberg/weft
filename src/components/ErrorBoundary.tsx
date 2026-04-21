import { Component, type ErrorInfo, type ReactNode } from "react";
import { AlertTriangle, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";

/**
 * Catches render-time errors in its subtree and renders a fallback
 * instead of letting the crash propagate to the root (which, in Tauri,
 * leaves a blank white WebView and no recovery path).
 *
 * Wraps each top-level route component in App.tsx so a crash in, say,
 * Monaco's diff view doesn't kill Sidebar + Toolbar too — the user can
 * still ⌘K to another task or reload.
 *
 * We don't ship a telemetry hook because weft is local-only. The error
 * is printed to the console so `bun run tauri dev` surfaces it during
 * development, and shown in the fallback so the user can copy it into a
 * bug report.
 */
interface Props {
  children: ReactNode;
  /** Label shown in the fallback — which surface crashed. */
  scope?: string;
  /** Key to reset the boundary when the parent's context changes.
   *  Setting this to e.g. a route id makes "navigate elsewhere" clear
   *  the error automatically without requiring a manual reload. */
  resetKey?: string | number;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // Log to console so the Tauri dev terminal shows the stack. Don't
    // swallow — the raw stack is most useful.
    console.error("[ErrorBoundary]", this.props.scope ?? "", error, info);
  }

  componentDidUpdate(prev: Props) {
    if (this.state.error && prev.resetKey !== this.props.resetKey) {
      // Navigated away / surface changed — fresh start.
      this.setState({ error: null });
    }
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;

    return (
      <div className="flex h-full flex-col items-center justify-center p-8">
        <div className="border-destructive/30 bg-destructive/5 w-full max-w-md rounded-lg border p-5">
          <div className="mb-3 flex items-center gap-2">
            <AlertTriangle size={16} className="text-destructive" />
            <h2 className="text-sm font-semibold">
              Something broke{this.props.scope ? ` in ${this.props.scope}` : ""}
            </h2>
          </div>
          <p className="text-muted-foreground mb-3 text-xs">
            The error is caught here so the rest of the app keeps working.
            Try reloading the view; if it recurs, the message below is
            what to share in a bug report.
          </p>
          <pre className="bg-background border-border text-destructive mb-4 max-h-40 overflow-auto rounded border p-2 font-mono text-[11px]">
            {error.message}
          </pre>
          <div className="flex gap-2">
            <Button
              size="sm"
              variant="outline"
              onClick={() => this.setState({ error: null })}
              className="gap-1 text-xs"
            >
              <RefreshCw size={12} />
              Reset this view
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => window.location.reload()}
              className="text-xs"
            >
              Reload app
            </Button>
          </div>
        </div>
      </div>
    );
  }
}
