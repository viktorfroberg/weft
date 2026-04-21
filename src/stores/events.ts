import { create } from "zustand";
import type { DbEvent } from "@/lib/events";

interface LoggedEvent extends DbEvent {
  ts: number;
}

interface EventLogState {
  events: LoggedEvent[];
  push: (e: DbEvent) => void;
  clear: () => void;
}

const MAX_KEEP = 50;

export const useEventLog = create<EventLogState>((set) => ({
  events: [],
  push: (e) =>
    set((s) => ({
      events: [{ ...e, ts: Date.now() }, ...s.events].slice(0, MAX_KEEP),
    })),
  clear: () => set({ events: [] }),
}));
