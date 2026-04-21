/**
 * Terminal bell helpers.
 *
 *   - `playBell()` — 880Hz sine beep via Web Audio. Synthesized on the
 *     fly, no bundle cost, no file to ship. AudioContext is lazy-inited
 *     on first call; browsers block creation until a user gesture,
 *     which is fine because bells only fire during terminal sessions
 *     that started from one (or from a "Test bell" button click).
 *
 *   - `fireTestBell()` / `onTestBell()` — a tiny pub/sub channel so the
 *     Settings "Test bell" button can reach the `TerminalPreview`'s
 *     visual-flash state without needing a PTY or a shared global.
 *     Listeners register on mount and get fired on the active
 *     bellStyle; the button reads the latest bellStyle at click time.
 */
let ctx: AudioContext | null = null;

function getContext(): AudioContext | null {
  if (ctx) return ctx;
  if (typeof window === "undefined") return null;
  const Ctor = window.AudioContext ?? (window as unknown as {
    webkitAudioContext?: typeof AudioContext;
  }).webkitAudioContext;
  if (!Ctor) return null;
  try {
    ctx = new Ctor();
  } catch {
    return null;
  }
  return ctx;
}

// ---------------------------------------------------------------------------
// Test-bell pub/sub — lets the Settings button fire a bell in any mounted
// TerminalPreview without plumbing props across the Settings tree.
// ---------------------------------------------------------------------------

type TestListener = () => void;
const testListeners = new Set<TestListener>();

/** Subscribe to test-bell events. Returns an unsubscribe fn. */
export function onTestBell(fn: TestListener): () => void {
  testListeners.add(fn);
  return () => testListeners.delete(fn);
}

/** Dispatch a test bell — all live listeners fire (typically just the
 * one `TerminalPreview` in Settings). Called from the "Test bell"
 * button. */
export function fireTestBell() {
  for (const fn of testListeners) fn();
}

/** Play the audible bell once. Safe to call rapidly — each call makes
 * its own oscillator + gain node, disposed after stop. */
export function playBell(frequency = 880, durationMs = 80, volume = 0.1) {
  const audio = getContext();
  if (!audio) return;
  if (audio.state === "suspended") {
    audio.resume().catch(() => {});
  }
  const now = audio.currentTime;
  const osc = audio.createOscillator();
  const gain = audio.createGain();
  osc.frequency.value = frequency;
  osc.type = "sine";
  gain.gain.setValueAtTime(0, now);
  gain.gain.linearRampToValueAtTime(volume, now + 0.005);
  gain.gain.linearRampToValueAtTime(0, now + durationMs / 1000);
  osc.connect(gain).connect(audio.destination);
  osc.start(now);
  osc.stop(now + durationMs / 1000 + 0.02);
}
