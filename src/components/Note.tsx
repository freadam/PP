/**
 * A task note, shown as what it is (§1, §2, M4).
 *
 * This replaces a small Markdown renderer. The spec puts "no personal-notes
 * system, wiki, Markdown editor" out of scope and asks for *one compact
 * plain-text note*, and `ROADMAP.md` named the renderer as a live instance of
 * scope creep. So: `**bold**` renders as those nine characters, because that is
 * what they always were. Nothing anyone wrote has changed — only the
 * interpretation has, and the text is right there to be re-read.
 *
 * ── The one exception, and why it is not markup ───────────────────────
 * A bare `https://…` becomes a link. That is not a markup language and it does
 * not invite one: there is no syntax to learn, nothing to escape, and no way to
 * write a link that displays as different text — which is precisely the feature
 * that turns a note field into a document format. A URL you can see and cannot
 * open is just an annoyance with no argument behind it.
 *
 * As before, nothing here goes near `dangerouslySetInnerHTML`. The renderer
 * sits one IPC call away from the user's database.
 */

import type { ReactNode } from "react";

/** Bare http(s) URLs. Trailing punctuation is left out of the link — a URL at
 *  the end of a sentence should not swallow the full stop. */
const URL_PATTERN = /https?:\/\/[^\s<>"']+[^\s<>"'.,;:!?)\]]/g;

export function Note({ source }: { source: string }) {
  if (!source.trim()) {
    return <p className="caption">Nothing written yet.</p>;
  }

  const out: ReactNode[] = [];
  let last = 0;
  let match: RegExpExecArray | null;
  let i = 0;
  URL_PATTERN.lastIndex = 0;

  while ((match = URL_PATTERN.exec(source))) {
    if (match.index > last) out.push(source.slice(last, match.index));
    out.push(
      <a
        key={`u${i++}`}
        href={match[0]}
        rel="noreferrer noopener"
        style={{ color: "var(--plot)" }}
      >
        {match[0]}
      </a>,
    );
    last = match.index + match[0].length;
  }
  if (last < source.length) out.push(source.slice(last));

  // `pre-wrap` is the whole layout: line breaks are where you put them, and
  // there is no block structure to compute because there is no block syntax.
  return (
    <div className="note-body" style={{ whiteSpace: "pre-wrap" }}>
      {out}
    </div>
  );
}
