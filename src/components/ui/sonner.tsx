import { Toaster as SonnerToaster } from "sonner";
import { useEffectiveTheme } from "@/lib/theme";

/**
 * App-wide toast host. Mount once at the App root.
 *
 * Sonner is chosen over shadcn's `toast` because it's lower-ceremony
 * (imperative `toast.success("...")` from anywhere) and has better
 * stacking behavior out of the box. Theme is synced to weft's
 * `useEffectiveTheme` so the toasts don't look out of place when the
 * user flips themes.
 */
export function Toaster() {
  const theme = useEffectiveTheme();
  return (
    <SonnerToaster
      theme={theme}
      position="bottom-right"
      richColors
      closeButton
      duration={3500}
      toastOptions={{
        classNames: {
          toast:
            "group toast border-border bg-background text-foreground shadow-lg",
          description: "text-muted-foreground",
          actionButton: "bg-primary text-primary-foreground",
          cancelButton: "bg-muted text-muted-foreground",
        },
      }}
    />
  );
}
