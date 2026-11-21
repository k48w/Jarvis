import { useEffect, useRef, useState } from "react";
import { Play, Square, Terminal, Trash2 } from "lucide-react";

export interface TerminalLine {
  kind: "cmd" | "stdout" | "stderr" | "meta";
  text: string;
}

export interface TerminalSession {
  id: string;
  command: string;
  cwd: string;
  running: boolean;
  exitCode?: number;
  cancelled?: boolean;
  lines: TerminalLine[];
}

const QUICK_COMMANDS = [
  "git status",
  "git log -n 5 --oneline",
  "npm test",
  "Get-ChildItem",
];

export function TerminalView({
  sessions,
  projectPath,
  onRun,
  onCancel,
  onClear,
}: {
  sessions: TerminalSession[];
  projectPath?: string | null;
  onRun: (command: string, cwd?: string) => Promise<void>;
  onCancel: (id: string) => Promise<void>;
  onClear: () => void;
}) {
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const bottomRef = useRef<HTMLDivElement | null>(null);

  const runningSession = sessions.find((s) => s.running) ?? null;

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [sessions]);

  async function handleSubmit(cmdText?: string) {
    const cmd = (cmdText ?? input).trim();
    if (!cmd || busy) return;
    setBusy(true);
    try {
      if (!cmdText) setInput("");
      await onRun(cmd, projectPath ?? undefined);
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="panel-shell terminal-shell" aria-label="Terminal ao vivo">
      <div className="panel-header">
        <div>
          <h2>
            <Terminal size={17} style={{ verticalAlign: "-3px", marginRight: 6 }} />
            Terminal ao Vivo
          </h2>
          <p>
            Saída em streaming em tempo real no workspace{" "}
            <code>{projectPath || "Pasta do Usuário"}</code>.
          </p>
        </div>
        <div style={{ display: "flex", gap: 8 }}>
          {runningSession && (
            <button
              type="button"
              className="mini-btn danger"
              onClick={() => void onCancel(runningSession.id)}
            >
              <Square size={12} /> Parar
            </button>
          )}
          <button type="button" className="mini-btn" onClick={onClear}>
            <Trash2 size={12} /> Limpar
          </button>
        </div>
      </div>

      <div className="terminal-quick-chips">
        {QUICK_COMMANDS.map((cmd) => (
          <button
            key={cmd}
            type="button"
            className="mini-btn"
            onClick={() => void handleSubmit(cmd)}
          >
            $ {cmd}
          </button>
        ))}
      </div>

      <div className="terminal-output" role="log" aria-live="polite">
        {sessions.length === 0 ? (
          <div className="terminal-empty">
            Nenhum comando executado ainda. Digite um comando PowerShell abaixo ou peça ao J.A.R.V.I.S. para rodar testes, builds ou scripts.
          </div>
        ) : (
          sessions.map((s) => (
            <div key={s.id} className="terminal-block">
              <div className="terminal-cmd-header">
                <span className="terminal-prompt">❯ {s.command}</span>
                <span
                  className={
                    s.running
                      ? "terminal-badge running"
                      : s.exitCode === 0
                        ? "terminal-badge ok"
                        : "terminal-badge err"
                  }
                >
                  {s.running
                    ? "RODANDO..."
                    : s.cancelled
                      ? "CANCELADO"
                      : `EXIT ${s.exitCode ?? 0}`}
                </span>
              </div>
              <div className="terminal-lines">
                {s.lines.map((l, idx) => (
                  <div key={idx} className={`term-line ${l.kind}`}>
                    {l.text || " "}
                  </div>
                ))}
              </div>
            </div>
          ))
        )}
        <div ref={bottomRef} />
      </div>

      <form
        className="terminal-input-bar"
        onSubmit={(e) => {
          e.preventDefault();
          void handleSubmit();
        }}
      >
        <span className="terminal-input-prefix">PS ❯</span>
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="Digite um comando PowerShell (ex: npm run build, git status, pytest)..."
        />
        <button type="submit" className="mini-btn accent" disabled={busy || !input.trim()}>
          <Play size={12} /> Executar
        </button>
      </form>
    </section>
  );
}
