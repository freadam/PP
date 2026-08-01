/**
 * The two questions a block can ask (§4.3, P2).
 *
 * **Repeat** picks a rule. The presets come from Rust — `get_rrule_presets`
 * describes them with the same code that parses them, so the picker can never
 * offer a rule the engine would refuse.
 *
 * **Scope** is the one that matters. Removing an occurrence of a series is the
 * operation every calendar gets wrong by guessing, so Fruit asks: this one,
 * this and later, or all of them. There is no default button and no Enter
 * shortcut — the three answers are genuinely different, and one of them throws
 * away months of plotted intentions.
 */

import { useEffect, useState } from "react";
import { useApp, undoableDelete } from "../store/app";
import * as ipc from "../lib/ipc";
import type { RrulePreset, SeriesScope } from "../lib/types";

export function BlockDialogs() {
  const dialog = useApp((s) => s.blockDialog);
  if (!dialog) return null;
  return dialog.kind === "repeat" ? (
    <RepeatDialog blockId={dialog.blockId} title={dialog.title} />
  ) : (
    <ScopeDialog blockId={dialog.blockId} title={dialog.title} />
  );
}

function Frame({ label, children }: { label: string; children: React.ReactNode }) {
  const close = useApp((s) => s.setBlockDialog);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        close(null);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [close]);

  return (
    <>
      <div className="scrim" onClick={() => close(null)} />
      <div className="modal overlay" role="dialog" aria-modal="true" aria-label={label}>
        {children}
      </div>
    </>
  );
}

function RepeatDialog({ blockId, title }: { blockId: string; title: string }) {
  const close = useApp((s) => s.setBlockDialog);
  const run = useApp((s) => s.run);
  const refresh = useApp((s) => s.refresh);
  const toast = useApp((s) => s.toast);
  const [presets, setPresets] = useState<RrulePreset[]>([]);

  useEffect(() => {
    void ipc.getRrulePresets().then(setPresets).catch(() => setPresets([]));
  }, []);

  const apply = async (preset: RrulePreset) => {
    const blocks = await run(
      () => ipc.repeatBlock(blockId, preset.rrule),
      "Couldn't make that block repeat.",
    );
    close(null);
    if (blocks) {
      // Say how many were created: a rule that silently produces nothing (a
      // monthly 31st, say) would otherwise look like it worked.
      toast(`${preset.description} — ${blocks.length - 1} more plotted over the next 90 days.`);
      await refresh();
    }
  };

  return (
    <Frame label="Repeat block">
      <div className="sheet-head">
        <strong className="title">Repeat “{title}”</strong>
      </div>
      <div className="sheet-body stack">
        <p className="caption">
          Occurrences are real blocks, not a rule drawn on the calendar: each one can be moved,
          tracked against and reconciled on its own. They are plotted 90 days ahead and topped up
          as you scroll.
        </p>
        {presets.length === 0 ? (
          <p className="caption">Repeat presets aren't available in browser preview.</p>
        ) : (
          <div className="stack" style={{ gap: 4 }}>
            {presets.map((p) => (
              <button key={p.rrule} className="btn" onClick={() => void apply(p)}>
                <span className="grow" style={{ textAlign: "left" }}>
                  {p.label}
                </span>
                <span className="micro" style={{ color: "var(--muted)" }}>
                  {p.description}
                </span>
              </button>
            ))}
          </div>
        )}
        <div className="row">
          <span className="grow" />
          <button className="btn" onClick={() => close(null)}>
            Cancel <span className="kbd">Esc</span>
          </button>
        </div>
      </div>
    </Frame>
  );
}

function ScopeDialog({ blockId, title }: { blockId: string; title: string }) {
  const close = useApp((s) => s.setBlockDialog);
  const run = useApp((s) => s.run);
  const refresh = useApp((s) => s.refresh);
  const selectBlock = useApp((s) => s.selectBlock);

  const remove = async (scope: SeriesScope) => {
    const token = await run(
      () => ipc.unscheduleSeries(blockId, scope),
      "Couldn't remove that occurrence.",
    );
    close(null);
    if (token) {
      // Undo restores the whole scope in one step, including "all of them" —
      // which is what makes asking safe rather than merely careful.
      undoableDelete(token.label, token);
      selectBlock(null);
      await refresh();
    }
  };

  return (
    <Frame label="Remove from series">
      <div className="sheet-head">
        <strong className="title">“{title}” repeats</strong>
      </div>
      <div className="sheet-body stack">
        <p className="caption">Which occurrences should go? Any of these can be undone.</p>
        <div className="stack" style={{ gap: 4 }}>
          <button className="btn" onClick={() => void remove("instance")}>
            <span className="grow" style={{ textAlign: "left" }}>
              Just this one
            </span>
          </button>
          <button className="btn" onClick={() => void remove("future")}>
            <span className="grow" style={{ textAlign: "left" }}>
              This and everything after it
            </span>
          </button>
          <button className="btn btn-danger" onClick={() => void remove("all")}>
            <span className="grow" style={{ textAlign: "left" }}>
              The whole series
            </span>
          </button>
        </div>
        <div className="row">
          <span className="grow" />
          <button className="btn" onClick={() => close(null)}>
            Cancel <span className="kbd">Esc</span>
          </button>
        </div>
      </div>
    </Frame>
  );
}
