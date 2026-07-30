import { useEffect } from "react";
import { useApp } from "./store/app";
import { useKeyboard } from "./lib/useKeyboard";
import * as ipc from "./lib/ipc";
import {
  IdleBanner,
  NavRail,
  PreviewNotice,
  RecoveryModal,
  Toasts,
  TopBar,
} from "./components/chrome";
import { Sidebar } from "./components/Sidebar";
import { CommandPalette, ShortcutSheet } from "./components/Palette";
import { Planner } from "./views/Planner";
import { Tasks } from "./views/Tasks";
import { TaskDetail } from "./views/TaskDetail";
import { Reports } from "./views/Reports";
import { Activity, Settings } from "./views/Settings";
import { Focus } from "./views/Focus";
import { ReconcileSheet } from "./views/Reconcile";
import type { TimerState } from "./lib/types";
import { BREAK_DETAIL_COLUMN, useViewportWidth } from "./lib/useViewport";

/** §5.8 — the detail panel is a third column at ≥1280px, an overlay sheet below. */
function useDetailMode(): "column" | "sheet" {
  return useViewportWidth() >= BREAK_DETAIL_COLUMN ? "column" : "sheet";
}

export default function App() {
  const view = useApp((s) => s.view);
  const overlay = useApp((s) => s.overlay);
  const detail = useApp((s) => s.detail);
  const boot = useApp((s) => s.boot);
  const setTimer = useApp((s) => s.setTimer);
  const detailMode = useDetailMode();

  useKeyboard();

  useEffect(() => {
    void boot();
  }, [boot]);

  /* Events pushed from Rust — the renderer never polls (§6.8). Elapsed time in
     particular never comes from a renderer setInterval: Rust owns the monotonic
     accumulator, and the renderer only formats. That is what makes sleep and
     clock-change correctness possible at all (§6.9). */
  useEffect(() => {
    const unsubs: Array<() => void> = [];
    void ipc.listen<TimerState>("timer:tick", setTimer).then((u) => unsubs.push(u));
    void ipc.listen<TimerState>("timer:state", setTimer).then((u) => unsubs.push(u));
    void ipc
      .listen<TimerState>("timer:idle-detected", setTimer)
      .then((u) => unsubs.push(u));
    void ipc
      .listen<TimerState>("timer:recovery-required", setTimer)
      .then((u) => unsubs.push(u));
    void ipc
      .listen<string>("backup:failed", (why) =>
        useApp
          .getState()
          .toast(`Couldn't write today's backup. ${why} Open Settings → Data.`, {
            tone: "danger",
          }),
      )
      .then((u) => unsubs.push(u));
    void ipc
      .listen<string>("db:integrity-failed", (why) =>
        useApp
          .getState()
          .toast(`The database failed its integrity check. ${why}`, { tone: "danger" }),
      )
      .then((u) => unsubs.push(u));
    return () => unsubs.forEach((u) => u());
  }, [setTimer]);

  // Flush the note buffer before the window goes away (§6.7 crash flush).
  useEffect(() => {
    const flush = () => void useApp.getState().flushNote();
    window.addEventListener("beforeunload", flush);
    document.addEventListener("visibilitychange", flush);
    return () => {
      window.removeEventListener("beforeunload", flush);
      document.removeEventListener("visibilitychange", flush);
    };
  }, []);

  return (
    <div className="shell">
      <TopBar />
      <div className="shell-body">
        <NavRail />
        <Sidebar />
        <main className="main">
          {view === "planner" && <Planner />}
          {view === "tasks" && <Tasks />}
          {view === "reports" && <Reports />}
          {view === "settings" && <Settings />}
          {view === "activity" && <Activity />}
        </main>
        {detail && detailMode === "column" && overlay !== "focus" && (
          <TaskDetail mode="column" />
        )}
      </div>

      {/* Overlays are mutually exclusive; Esc dismisses (§2.2). */}
      {overlay === "palette" && <CommandPalette />}
      {overlay === "shortcuts" && <ShortcutSheet />}
      {overlay === "reconcile" && <ReconcileSheet />}
      {overlay === "focus" && <Focus />}
      {detail && detailMode === "sheet" && overlay === "detail" && <TaskDetail mode="sheet" />}

      <RecoveryModal />
      <IdleBanner />
      <Toasts />
      <PreviewNotice />
    </div>
  );
}
