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
import { Settings } from "./views/Settings";
import { Activity } from "./views/Activity";
import { Day } from "./views/Day";
import { ExcelExport } from "./views/ExcelExport";
import { Focus } from "./views/Focus";
import { ReconcileSheet } from "./views/Reconcile";
import { BlockDialogs } from "./components/BlockDialogs";
import type { Notice, TimerState } from "./lib/types";
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
    /* §3.5 — the recording indicator is driven by the sampler itself, not by a
       renderer timer: it lights because a sample was actually written, which is
       the only honest reason to tell someone they are being recorded. */
    void ipc
      .listen<void>("activity:sampled", () => useApp.getState().noteSample())
      .then((u) => unsubs.push(u));
    /* W4/W5 — a notice, never a nag. It arrives as an ordinary toast: nothing
       is interrupted, nothing is blocked, and it carries the one action that
       matters, which is making it stop. */
    void ipc
      .listen<Notice>("notice", (n) =>
        useApp.getState().toast(`${n.title} — ${n.body}`, {
          action: {
            label: "Quiet for 30m",
            run: () => void ipc.silenceNotices(30),
          },
        }),
      )
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
          {view === "day" && <Day />}
          {view === "planner" && <Planner />}
          {view === "tasks" && <Tasks />}
          {view === "reports" && <Reports />}
          {view === "settings" && <Settings />}
          {view === "export" && <ExcelExport />}
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

      <BlockDialogs />
      <RecoveryModal />
      <IdleBanner />
      <Toasts />
      <PreviewNotice />
    </div>
  );
}
