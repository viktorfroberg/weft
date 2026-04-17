function App() {
  return (
    <div className="grid h-screen grid-cols-[240px_1fr] bg-[var(--color-bg)]">
      <aside className="flex flex-col border-r border-[var(--color-border)] bg-[var(--color-surface)]">
        <header
          data-tauri-drag-region
          className="px-4 pt-3.5 pb-2.5 pl-20 font-semibold tracking-wide"
        >
          weft
        </header>
        <nav className="flex-1 overflow-y-auto px-2 py-2">
          <div className="px-2 pt-2.5 pb-1.5 text-[11px] uppercase tracking-wider text-[var(--color-muted)]">
            Workspaces
          </div>
          <div className="px-2 py-2 italic text-[var(--color-subtle)]">
            No workspaces yet
          </div>
        </nav>
      </aside>
      <main className="flex flex-col overflow-hidden">
        <header className="border-b border-[var(--color-border)] px-4 py-2.5 text-xs">
          <span className="text-[var(--color-muted)]">Ready</span>
        </header>
        <section className="flex flex-1 items-center justify-center p-6">
          <div className="max-w-md text-center">
            <h1 className="mb-2 text-3xl font-semibold">weft</h1>
            <p className="text-[var(--color-muted)]">
              Multi-repo agent orchestration.
            </p>
            <p className="text-xs text-[var(--color-muted)]">
              Phase 0 shell — workspace UI lands in Phase 3.
            </p>
          </div>
        </section>
      </main>
    </div>
  );
}

export default App;
