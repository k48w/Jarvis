import { useState, type ReactNode } from "react";
import { Check, Copy, Volume2 } from "lucide-react";

interface Block {
  type: "code" | "text";
  lang?: string;
  content: string;
}

function splitBlocks(raw: string): Block[] {
  const blocks: Block[] = [];
  const regex = /```([a-zA-Z0-9_-]*)\n?([\s\S]*?)```/g;
  let last = 0;
  for (const match of raw.matchAll(regex)) {
    const idx = match.index ?? 0;
    if (idx > last) {
      const before = raw.slice(last, idx).trim();
      if (before) blocks.push({ type: "text", content: before });
    }
    blocks.push({
      type: "code",
      lang: match[1]?.trim() || "código",
      content: match[2].replace(/\n$/, ""),
    });
    last = idx + match[0].length;
  }
  if (last < raw.length) {
    const rest = raw.slice(last).trim();
    if (rest) blocks.push({ type: "text", content: rest });
  }
  return blocks;
}

function renderInline(line: string, keyPrefix: string): ReactNode[] {
  const parts: ReactNode[] = [];
  const regex = /(\*\*[^*]+\*\*|`[^`]+`|\*[^*]+\*)/g;
  let last = 0;
  let k = 0;
  for (const m of line.matchAll(regex)) {
    const idx = m.index ?? 0;
    if (idx > last) parts.push(line.slice(last, idx));
    const token = m[0];
    if (token.startsWith("**")) {
      parts.push(<strong key={`${keyPrefix}-b-${k++}`}>{token.slice(2, -2)}</strong>);
    } else if (token.startsWith("`")) {
      parts.push(<code key={`${keyPrefix}-c-${k++}`}>{token.slice(1, -1)}</code>);
    } else if (token.startsWith("*")) {
      parts.push(<em key={`${keyPrefix}-i-${k++}`}>{token.slice(1, -1)}</em>);
    }
    last = idx + token.length;
  }
  if (last < line.length) parts.push(line.slice(last));
  return parts;
}

function CodeBlock({ lang, content }: { lang: string; content: string }) {
  const [copied, setCopied] = useState(false);

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(content);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      /* ignore */
    }
  }

  return (
    <div className="hud-code">
      <div className="hud-code-head">
        <span>{lang}</span>
        <button type="button" className="hud-copy-btn" onClick={() => void handleCopy()}>
          {copied ? <Check /> : <Copy />}
          {copied ? "Copiado" : "Copiar"}
        </button>
      </div>
      <pre>
        <code>{content}</code>
      </pre>
    </div>
  );
}

export function MarkdownBlocks({ text }: { text: string }) {
  const blocks = splitBlocks(text);
  return (
    <>
      {blocks.map((block, bIdx) => {
        if (block.type === "code") {
          return (
            <CodeBlock
              key={`code-${bIdx}`}
              lang={block.lang ?? "código"}
              content={block.content}
            />
          );
        }

        const lines = block.content.split(/\r?\n/);
        return (
          <div key={`txt-${bIdx}`} className="response-text-block">
            {lines.map((line, lIdx) => {
              const trimmed = line.trim();
              if (!trimmed) return null;
              if (/^#{1,3}\s+/.test(trimmed)) {
                return (
                  <h4 key={`h-${bIdx}-${lIdx}`} className="hud-md-heading">
                    {renderInline(trimmed.replace(/^#{1,3}\s+/, ""), `${bIdx}-${lIdx}`)}
                  </h4>
                );
              }
              if (/^[-*•]\s+/.test(trimmed)) {
                return (
                  <div key={`li-${bIdx}-${lIdx}`} className="hud-bullet">
                    <span className="hud-bullet-dot" />
                    <span>{renderInline(trimmed.replace(/^[-*•]\s+/, ""), `${bIdx}-${lIdx}`)}</span>
                  </div>
                );
              }
              return (
                <p key={`p-${bIdx}-${lIdx}`}>
                  {renderInline(trimmed, `${bIdx}-${lIdx}`)}
                </p>
              );
            })}
          </div>
        );
      })}
    </>
  );
}

export function ResponseCard({
  text,
  streaming,
  speaking,
  canReplay,
  onReplay,
}: {
  text: string;
  streaming: boolean;
  speaking: boolean;
  canReplay: boolean;
  onReplay: () => void;
}) {
  const [copied, setCopied] = useState(false);
  if (!text) return null;

  async function handleCopyAll() {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      /* ignore */
    }
  }

  return (
    <div className="subtitle response-hud" aria-live="polite">
      <div className="response-hud-head">
        <span className="response-tag">
          <i className={speaking || streaming ? "live" : ""} />
          J.A.R.V.I.S.
          {streaming ? " · transmitindo" : speaking ? " · falando" : ""}
        </span>
        <div className="response-actions">
          {canReplay && !streaming && (
            <button
              type="button"
              className="icon-tiny"
              aria-label="Ouvir resposta novamente"
              title="Ouvir novamente"
              onClick={onReplay}
            >
              <Volume2 />
            </button>
          )}
          <button
            type="button"
            className="icon-tiny"
            aria-label="Copiar resposta"
            title={copied ? "Copiado!" : "Copiar resposta"}
            onClick={() => void handleCopyAll()}
          >
            {copied ? <Check /> : <Copy />}
          </button>
        </div>
      </div>

      <div className="response-body">
        <MarkdownBlocks text={text} />
      </div>
    </div>
  );
}
