import { useState } from "react";
import { toast } from "sonner";
import { Upload } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { fontInstallPick } from "@/lib/commands";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** Custom-font installer. Why importing instead of "type a system font
 *  name": macOS's webview (the part that draws everything you see in
 *  weft) can only render fonts Apple ships by default — Helvetica,
 *  Menlo, Monaco, Courier. Fonts you've installed yourself via Font
 *  Book or `~/Library/Fonts/` (Maple Mono, MonoLisa, Berkeley Mono,
 *  Operator Mono, etc.) are deliberately invisible to the webview as a
 *  privacy / fingerprinting safeguard.
 *
 *  Workaround: copy the font file directly into weft's data dir and
 *  register it as a web font. The system file picker handles the rest.
 *
 *  This dialog is dead simple — backend does the validation +
 *  copy on confirm. */
export function AddSystemFontDialog({ open, onOpenChange }: Props) {
  const [pending, setPending] = useState(false);

  const pick = async () => {
    setPending(true);
    try {
      const row = await fontInstallPick();
      if (!row) {
        // User cancelled the picker — nothing to do.
        return;
      }
      toast.success(`Added ${row.display_name}`);
      onOpenChange(false);
    } catch (err) {
      toast.error("Couldn't add font", { description: String(err) });
    } finally {
      setPending(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-6 p-8 sm:max-w-lg">
        <DialogHeader className="space-y-3">
          <DialogTitle>Add custom font</DialogTitle>
          <DialogDescription>
            Pick a font file from your disk — typically{" "}
            <code className="bg-muted rounded px-1.5 py-0.5 text-xs">
              ~/Library/Fonts/
            </code>{" "}
            or wherever you downloaded it. Supports{" "}
            <code className="bg-muted rounded px-1.5 py-0.5 text-xs">
              .ttf
            </code>{" "}
            <code className="bg-muted rounded px-1.5 py-0.5 text-xs">
              .otf
            </code>{" "}
            <code className="bg-muted rounded px-1.5 py-0.5 text-xs">
              .ttc
            </code>{" "}
            <code className="bg-muted rounded px-1.5 py-0.5 text-xs">
              .woff
            </code>{" "}
            <code className="bg-muted rounded px-1.5 py-0.5 text-xs">
              .woff2
            </code>
            .
          </DialogDescription>
        </DialogHeader>

        <div className="bg-muted/40 border-border text-muted-foreground rounded-md border p-4 text-xs leading-relaxed">
          <span className="text-foreground font-medium">Why import?</span>{" "}
          macOS's webview hides third-party fonts you've installed via Font
          Book — only Apple-shipped families (Menlo, Monaco, Helvetica, …)
          are reachable by name. weft copies your font into its own data
          dir so the webview can actually load it. The original install in
          Font Book stays untouched.
        </div>

        <div className="flex justify-center py-2">
          <Button
            type="button"
            onClick={pick}
            disabled={pending}
            className="gap-2"
            size="lg"
          >
            <Upload size={16} />
            {pending ? "Working…" : "Pick font file…"}
          </Button>
        </div>

        <DialogFooter>
          <Button
            type="button"
            variant="ghost"
            onClick={() => onOpenChange(false)}
            disabled={pending}
          >
            Cancel
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
