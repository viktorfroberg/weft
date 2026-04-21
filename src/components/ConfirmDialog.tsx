import { create } from "zustand";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";

interface ConfirmRequest {
  title: string;
  description?: string;
  confirmText?: string;
  cancelText?: string;
  destructive?: boolean;
  resolve: (ok: boolean) => void;
}

interface ConfirmState {
  current: ConfirmRequest | null;
  ask: (opts: Omit<ConfirmRequest, "resolve">) => Promise<boolean>;
  resolve: (ok: boolean) => void;
}

const useConfirmStore = create<ConfirmState>((set, get) => ({
  current: null,
  ask: (opts) =>
    new Promise<boolean>((resolve) => {
      set({ current: { ...opts, resolve } });
    }),
  resolve: (ok) => {
    const { current } = get();
    if (current) {
      current.resolve(ok);
      set({ current: null });
    }
  },
}));

/** Imperative hook — returns a function that opens the shared dialog. */
export function useConfirm() {
  return useConfirmStore((s) => s.ask);
}

/** Mount once at the App root. */
export function ConfirmDialogHost() {
  const current = useConfirmStore((s) => s.current);
  const resolve = useConfirmStore((s) => s.resolve);
  const open = current !== null;

  return (
    <AlertDialog open={open} onOpenChange={(next) => !next && resolve(false)}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{current?.title}</AlertDialogTitle>
          {current?.description && (
            <AlertDialogDescription>
              {current.description}
            </AlertDialogDescription>
          )}
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel onClick={() => resolve(false)}>
            {current?.cancelText ?? "Cancel"}
          </AlertDialogCancel>
          <AlertDialogAction
            onClick={() => resolve(true)}
            className={
              current?.destructive
                ? "bg-destructive text-destructive-foreground hover:bg-destructive/90"
                : undefined
            }
          >
            {current?.confirmText ?? "Continue"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
