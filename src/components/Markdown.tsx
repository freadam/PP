/**
 * A deliberately small markdown renderer (§7.3, D12).
 *
 * It builds React elements. It never calls `dangerouslySetInnerHTML`, and it
 * never parses raw HTML — so a note pasted from a web page containing
 * `<img src=x onerror=…>` renders as the literal text it is. That matters more
 * here than in most apps: the renderer sits one IPC call away from the user's
 * database.
 *
 * The supported subset is what a note actually needs. Anything else stays
 * literal, which is the safe failure mode.
 */

import type { ReactNode } from "react";

const SAFE_PROTOCOL = /^(https?:|mailto:)/i;

function inline(text: string, keyBase: string): ReactNode[] {
  const out: ReactNode[] = [];
  // `code` · **bold** · *italic* · [label](url)
  const pattern = /(`[^`]+`)|(\*\*[^*]+\*\*)|(\*[^*]+\*)|(\[[^\]]+\]\([^)\s]+\))/g;
  let last = 0;
  let match: RegExpExecArray | null;
  let i = 0;

  while ((match = pattern.exec(text))) {
    if (match.index > last) out.push(text.slice(last, match.index));
    const token = match[0];
    const key = `${keyBase}-${i++}`;
    if (token.startsWith("`")) {
      out.push(
        <code key={key} className="data">
          {token.slice(1, -1)}
        </code>,
      );
    } else if (token.startsWith("**")) {
      out.push(<strong key={key}>{token.slice(2, -2)}</strong>);
    } else if (token.startsWith("[")) {
      const split = token.indexOf("](");
      const label = token.slice(1, split);
      const href = token.slice(split + 2, -1);
      out.push(
        SAFE_PROTOCOL.test(href) ? (
          <a key={key} href={href} rel="noreferrer noopener" style={{ color: "var(--plot)" }}>
            {label}
          </a>
        ) : (
          // An unrecognised protocol (javascript:, data:, file:) is shown, not linked.
          <span key={key}>{label}</span>
        ),
      );
    } else {
      out.push(<em key={key}>{token.slice(1, -1)}</em>);
    }
    last = match.index + token.length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

export function Markdown({ source }: { source: string }) {
  if (!source.trim()) {
    return <p className="caption">Nothing written yet.</p>;
  }

  const blocks: ReactNode[] = [];
  const lines = source.split("\n");
  let list: string[] = [];

  const flushList = (key: string) => {
    if (!list.length) return;
    blocks.push(
      <ul key={key} style={{ margin: "0 0 12px", paddingLeft: 20 }}>
        {list.map((item, i) => (
          <li key={i}>{inline(item, `${key}-${i}`)}</li>
        ))}
      </ul>,
    );
    list = [];
  };

  lines.forEach((line, index) => {
    const key = `b${index}`;
    const bullet = /^\s*[-*]\s+(.*)$/.exec(line);
    if (bullet) {
      list.push(bullet[1] ?? "");
      return;
    }
    flushList(`${key}-list`);

    const heading = /^(#{1,3})\s+(.*)$/.exec(line);
    if (heading) {
      const level = heading[1]!.length;
      const text = heading[2] ?? "";
      const style = { margin: "12px 0 6px", fontSize: level === 1 ? "1.125rem" : "1rem" };
      blocks.push(
        level === 1 ? (
          <h3 key={key} style={style}>
            {inline(text, key)}
          </h3>
        ) : (
          <h4 key={key} style={style}>
            {inline(text, key)}
          </h4>
        ),
      );
      return;
    }
    if (line.trim() === "") return;
    blocks.push(
      <p key={key} style={{ margin: "0 0 8px" }}>
        {inline(line, key)}
      </p>,
    );
  });
  flushList("tail-list");

  return <div className="markdown">{blocks}</div>;
}
