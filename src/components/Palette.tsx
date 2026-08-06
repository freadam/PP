/**
 * Command palette (§5.7) and the shortcut sheet (§4.2).
 *
 * Fuzzy match over commands, tasks, projects and tags, with the matched
 * substring in `--plot`. Both surfaces read the single command registry, which
 * is what makes "every action is reachable from the palette and from a
 * documented key" (U1) a structural fact rather than a promise.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { useApp } from "../store/app";
import { COMMANDS } from "../lib/commands";
import type { Command } from "../lib/commands";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type { SearchHit } from "../lib/types";

interface Row {
  key: string;
  title: string;
  subtitle?: string;
  hint?: string;
  run: () => void;
  match?: [number, number];
}

/** Subsequence match, the cheap fuzzy that behaves predictably. */
function fuzzy(haystack: string, needle: string): number | null {
  if (!needle) return 0;
  const h = haystack.toLowerCase();
  const n = needle.toLowerCase();
  const direct = h.indexOf(n);
  if (direct >= 0) return direct;
  let i = 0;
  for (const ch of n) {
    i = h.indexOf(ch, i);
    if (i < 0) return null;
    i += 1;
  }
  return 1000;
}

export function CommandPalette() {
  const close = useApp((s) => s.setOverlay);
  const openDetail = useApp((s) => s.openDetail);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const [hits, setHits] = useState<SearchHit[]>([]);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (query.trim().length < 2) {
      setHits([]);
      return;
    }
    let live = true;
    // Budget: first results ≤ 50ms (§7.1).
    void ipc
      .search(query, 20)
      .then((r) => live && setHits(r.hits))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [query]);

  const rows: Row[] = useMemo(() => {
    const available = COMMANDS.filter((c: Command) => !c.when || c.when());
    const commandRows: Row[] = available
      .map((c) => ({ c, score: fuzzy(c.title, query) }))
      .filter((x) => x.score !== null)
      .sort((a, b) => (a.score ?? 0) - (b.score ?? 0))
      .map(({ c }) => ({
        key: `cmd:${c.id}`,
        title: c.title,
        subtitle: c.group,
        hint: c.keys,
        match: matchRange(c.title, query),
        run: () => {
          close(null);
          void c.run();
        },
      }));

    const hitRows: Row[] = hits.map((h) => ({
      key: `${h.kind}:${h.id}`,
      title: h.title,
      subtitle: h.subtitle ?? h.kind,
      match:
        h.matchFrom != null && h.matchTo != null ? [h.matchFrom, h.matchTo] : undefined,
      run: () => {
        close(null);
        if (h.kind === "task" || h.kind === "note") void openDetail(h.id);
      },
    }));

    return [...commandRows.slice(0, 12), ...hitRows].slice(0, 40);
  }, [query, hits, close, openDetail]);

  useEffect(() => setActive(0), [query]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => Math.min(rows.length - 1, i + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => Math.max(0, i - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      rows[active]?.run();
    }
  };

  useEffect(() => {
    listRef.current
      ?.querySelector<HTMLElement>(`[data-index="${active}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [active]);

  return (
    <>
      <div className="scrim" onClick={() => close(null)} />
      <div className="modal overlay" role="dialog" aria-modal="true" aria-label="Command palette">
        <input
          autoFocus
          className="palette-input"
          placeholder="Type a command, or search tasks, notes and projects…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onKeyDown}
          aria-controls="palette-list"
        />
        <div className="palette-list" id="palette-list" ref={listRef} role="listbox">
          {rows.length === 0 ? (
            <p className="caption" style={{ padding: 16 }}>
              No matches for <em>{query}</em>. Search covers task titles, notes, and project names.
            </p>
          ) : (
            rows.map((row, i) => (
              <button
                key={row.key}
                className="palette-item"
                data-active={i === active}
                data-index={i}
                role="option"
                aria-selected={i === active}
                onMouseEnter={() => setActive(i)}
                onClick={row.run}
              >
                <Highlighted text={row.title} range={row.match} />
                {row.subtitle && <span className="caption">{row.subtitle}</span>}
                {row.hint && <span className="hint kbd">{fmt.keys(row.hint)}</span>}
              </button>
            ))
          )}
        </div>
      </div>
    </>
  );
}

function matchRange(text: string, query: string): [number, number] | undefined {
  if (!query) return undefined;
  const i = text.toLowerCase().indexOf(query.toLowerCase());
  return i >= 0 ? [i, i + query.length] : undefined;
}

function Highlighted({ text, range }: { text: string; range?: [number, number] }) {
  if (!range) return <span>{text}</span>;
  return (
    <span>
      {text.slice(0, range[0])}
      <mark>{text.slice(range[0], range[1])}</mark>
      {text.slice(range[1])}
    </span>
  );
}

/** `?` — the complete, rebindable map (§4.2). */
export function ShortcutSheet() {
  const close = useApp((s) => s.setOverlay);
  const groups = ["Global", "Planner", "Tasks", "Timer", "Data"] as const;

  return (
    <>
      <div className="scrim" onClick={() => close(null)} />
      <div className="modal modal-wide overlay" role="dialog" aria-modal="true" aria-label="Shortcuts">
        <div className="sheet-head">
          <strong className="title grow">Keyboard</strong>
          <span className="kbd">Esc</span>
        </div>
        <div className="sheet-body" style={{ maxHeight: "60vh" }}>
          <p className="caption" style={{ marginTop: 0 }}>
            Every action is also in the command palette. Nothing in Fruit needs a pointer.
          </p>
          {groups.map((group) => {
            const rows = COMMANDS.filter((c) => c.group === group && c.keys);
            if (!rows.length) return null;
            return (
              <section key={group} style={{ marginBottom: 16 }}>
                <h3 className="label" style={{ color: "var(--muted)" }}>
                  {group}
                </h3>
                {rows.map((c) => (
                  <div key={c.id} className="row spread" style={{ padding: "2px 0" }}>
                    <span>{c.title}</span>
                    <span className="kbd">{fmt.keys(c.keys!)}</span>
                  </div>
                ))}
              </section>
            );
          })}
          <section>
            <h3 className="label" style={{ color: "var(--muted)" }}>
              Drag
            </h3>
            <div className="row spread">
              <span>Push later blocks down</span>
              <span className="kbd">Shift while dropping</span>
            </div>
            <div className="row spread">
              <span>Shorten to fit the gap</span>
              <span className="kbd">Alt while dropping</span>
            </div>
            <div className="row spread">
              <span>5-minute precision</span>
              <span className="kbd">Alt while dragging</span>
            </div>
            <div className="row spread">
              <span>Cancel a drag</span>
              <span className="kbd">Esc</span>
            </div>
          </section>
        </div>
      </div>
    </>
  );
}
