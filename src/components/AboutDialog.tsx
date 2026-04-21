import { useEffect, useState } from "react";
import { ExternalLink } from "lucide-react";
import { appInfo, type AppInfo } from "@/lib/commands";
import { useUi } from "@/stores/ui";
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import logoMark from "@/assets/logo-mark.png";

const REPO_URL = "https://github.com/viktorfroberg/weft";

/**
 * Custom "About weft" dialog. Replaces Tauri's default about panel
 * (generic folder icon + version). Triggered by the native macOS menu
 * `weft → About weft` — see `src-tauri/src/menu.rs` + `src/lib/menu.ts`.
 */
export function AboutDialog() {
  const open = useUi((s) => s.aboutOpen);
  const setOpen = useUi((s) => s.setAboutOpen);
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    if (!open || info) return;
    appInfo().then(setInfo).catch(() => {});
  }, [open, info]);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="sm:max-w-sm">
        <DialogTitle className="sr-only">About weft</DialogTitle>
        <DialogDescription className="sr-only">
          Version, mission, and link to the GitHub repository.
        </DialogDescription>
        <div className="flex flex-col items-center gap-3 py-2 text-center">
          <img
            src={logoMark}
            alt=""
            className="h-24 w-24 select-none"
            draggable={false}
          />
          <div>
            <h2 className="text-foreground text-lg font-semibold tracking-tight">
              weft
            </h2>
            {info && (
              <p className="text-muted-foreground font-mono text-xs">
                v{info.version}
              </p>
            )}
          </div>
          <p className="text-foreground max-w-[28ch] text-sm">
            Multi-repo agent orchestration.
          </p>
          <a
            href={REPO_URL}
            target="_blank"
            rel="noreferrer"
            className="text-muted-foreground hover:text-foreground inline-flex items-center gap-1 text-xs underline-offset-4 hover:underline"
          >
            github.com/viktorfroberg/weft
            <ExternalLink size={11} />
          </a>
        </div>
      </DialogContent>
    </Dialog>
  );
}
