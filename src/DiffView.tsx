import { useEffect, useMemo, useState } from "react";
import {
  Check,
  Copy,
  FileCode2,
  FilePlus2,
  GitBranch,
  GitCommitHorizontal,
  RefreshCw,
  RotateCcw,
  Trash2,
} from "lucide-react";
import {
  getGitRepoStatus,
  gitCommitWorkspace,
  type FileDiffEvent,
  type GitRepoStatus,
} from "./bridge";

export interface FileDiffRecord extends FileDiffEvent {
  reverted?: boolean;
}

interface DiffLine {
  kind: "context" | "add" | "del";
  oldLine?: number;
  newLine?: number;
  text: string;
}

function computeLineDiff(oldText: string, newText: string): {
  lines: DiffLine[];
  added: number;
  removed: number;
} {
  const a = oldText.length === 0 ? [] : oldText.replace(/\r\n/g, "\n").split("\n");
  const b = newText.length === 0 ? [] : newText.replace(/\r\n/g, "\n").split("\n");

  // Trim identical prefix & suffix for speed
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) {
    start++;
  }
  let endA = a.length - 1;
  let endB = b.length - 1;
  while (endA >= start && endB >= start && a[endA] === b[endB]) {
    endA--;
    endB--;
  }

  const midA = a.slice(start, endA + 1);
  const midB = b.slice(start, endB + 1);

  const lines: DiffLine[] = [];
  let added = 0;
  let removed = 0;

  // Prefix context (keep last 3 context lines so huge files stay clean)
  const prefixKeepStart = Math.max(0, start - 3);
  for (let i = prefixKeepStart; i < start; i++) {
    lines.push({ kind: "context", oldLine: i + 1, newLine: i + 1, text: a[i] });
  }

  if (midA.length * midB.length <= 240_000) {
    // Exact LCS DP on the changed middle region
    const m = midA.length;
    const n = midB.length;
    const dp: Uint32Array[] = Array.from({ length: m + 1 }, () => new Uint32Array(n + 1));
    for (let i = m - 1; i >= 0; i--) {
      for (let j = n - 1; j >= 0; j--) {
        if (midA[i] === midB[j]) {
          dp[i][j] = dp[i + 1][j + 1] + 1;
        } else {
          dp[i][j] = dp[i + 1][j] >= dp[i][j + 1] ? dp[i + 1][j] : dp[i][j + 1];
        }
      }
    }

    let i = 0;
    let j = 0;
    while (i < m && j < n) {
      if (midA[i] === midB[j]) {
        lines.push({
          kind: "context",
          oldLine: start + i + 1,
          newLine: start + j + 1,
          text: midA[i],
        });
        i++;
        j++;
      } else if (dp[i + 1][j] >= dp[i][j + 1]) {
        lines.push({ kind: "del", oldLine: start + i + 1, text: midA[i] });
        removed++;
        i++;
      } else {
        lines.push({ kind: "add", newLine: start + j + 1, text: midB[j] });
        added++;
        j++;
      }
    }
    while (i < m) {
      lines.push({ kind: "del", oldLine: start + i + 1, text: midA[i] });
      removed++;
      i++;
    }
    while (j < n) {
      lines.push({ kind: "add", newLine: start + j + 1, text: midB[j] });
      added++;
      j++;
    }
  } else {
    // Fast linear fallback for huge files
    for (let i = 0; i < midA.length; i++) {
      lines.push({ kind: "del", oldLine: start + i + 1, text: midA[i] });
      removed++;
    }
    for (let j = 0; j < midB.length; j++) {
      lines.push({ kind: "add", newLine: start + j + 1, text: midB[j] });
      added++;
    }
  }

  // Suffix context (keep first 3 lines after the change)
  const suffixCount = Math.min(3, a.length - (endA + 1));
  for (let k = 0; k < suffixCount; k++) {
    lines.push({
      kind: "context",
      oldLine: endA + 2 + k,
      newLine: endB + 2 + k,
      text: a[endA + 1 + k],
    });
  }

  return { lines, added, removed };
}

export function DiffView({
  diffs,
  projectPath,
  onRevert,
  onClearAll,
}: {
  diffs: FileDiffRecord[];
  projectPath?: string | null;
  onRevert: (diff: FileDiffRecord) => Promise<void>;
  onClearAll: () => void;
}) {
  const [selectedId, setSelectedId] = useState<string>(diffs[0]?.id ?? "");
  const [revertingId, setRevertingId] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [gitStatus, setGitStatus] = useState<GitRepoStatus | null>(null);
  const [gitBusy, setGitBusy] = useState(false);
  const [commitMsg, setCommitMsg] = useState("");
  const [gitNotice, setGitNotice] = useState<string | null>(null);

  async function refreshGit(targetPath: string) {
    setGitBusy(true);
    try {
      const st = await getGitRepoStatus(targetPath);
      setGitStatus(st);
    } catch {
      setGitStatus(null);
    } finally {
      setGitBusy(false);
    }
  }

  useEffect(() => {
    if (projectPath) {
      void refreshGit(projectPath);
    } else {
      setGitStatus(null);
    }
  }, [projectPath, diffs.length]);

  async function handleCommit() {
    if (!projectPath || !commitMsg.trim() || gitBusy) return;
    setGitBusy(true);
    setGitNotice(null);
    try {
      const res = await gitCommitWorkspace(projectPath, commitMsg.trim());
      setCommitMsg("");
      setGitNotice(res);
      await refreshGit(projectPath);
    } catch (e) {
      setGitNotice(`Erro: ${String(e)}`);
    } finally {
      setGitBusy(false);
    }
  }

  const selected =
    diffs.find((d) => d.id === selectedId) ?? diffs[0] ?? null;

  const computedList = useMemo(
    () =>
      diffs.map((d) => ({
        diff: d,
        stats: computeLineDiff(d.oldContent, d.newContent),
      })),
    [diffs],
  );

  const activeEntry =
    computedList.find((e) => e.diff.id === selected?.id) ??
    computedList[0] ??
    null;

  async function handleCopy() {
    if (!activeEntry) return;
    const patch = activeEntry.stats.lines
      .map((l) => `${l.kind === "add" ? "+" : l.kind === "del" ? "-" : " "}${l.text}`)
      .join("\n");
    try {
      await navigator.clipboard.writeText(patch);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      /* ignore */
    }
  }

  async function handleRevertClick(d: FileDiffRecord) {
    setRevertingId(d.id);
    try {
      await onRevert(d);
    } finally {
      setRevertingId(null);
    }
  }

  return (
    <section className="panel-shell diff-shell" aria-label="Alterações de código e arquivos">
      <div className="panel-header">
        <div>
          <h2>Alterações em Arquivos (Diff)</h2>
          <p>
            Revise cada linha modificada pelo J.A.R.V.I.S. nos seus projetos e desfaça com 1 clique se precisar.
          </p>
        </div>
        <button
          type="button"
          className="mini-btn"
          onClick={onClearAll}
          title="Limpar painel de diffs"
        >
          <Trash2 /> Limpar
        </button>
      </div>

      {projectPath && gitStatus?.isGitRepo && (
        <div className="git-workspace-bar" aria-label="Status Git do projeto">
          <div className="git-ws-meta">
            <span className="git-branch-pill">
              <GitBranch size={13} /> {gitStatus.branch}
            </span>
            <span className="git-changes-count">
              {gitStatus.changes.length === 0
                ? "Árvore limpa"
                : `${gitStatus.changes.length} arquivo(s) modificado(s)`}
            </span>
            {(gitStatus.ahead > 0 || gitStatus.behind > 0) && (
              <span className="git-sync-pill">
                ↑{gitStatus.ahead} ↓{gitStatus.behind}
              </span>
            )}
            <button
              type="button"
              className="mini-btn"
              disabled={gitBusy}
              onClick={() => void refreshGit(projectPath)}
              title="Atualizar status Git"
            >
              <RefreshCw size={12} />
            </button>
          </div>
          {gitStatus.changes.length > 0 && (
            <div className="git-commit-row">
              <input
                type="text"
                className="git-commit-input"
                placeholder="Mensagem do commit (ex: feat: nova melhoria)..."
                value={commitMsg}
                onChange={(e) => setCommitMsg(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void handleCommit();
                }}
              />
              <button
                type="button"
                className="mini-btn accent"
                disabled={gitBusy || !commitMsg.trim()}
                onClick={() => void handleCommit()}
              >
                <GitCommitHorizontal size={13} /> Commitar
              </button>
            </div>
          )}
          {gitNotice && <div className="git-notice">{gitNotice}</div>}
        </div>
      )}

      <div className="diff-layout">
        <aside className="diff-sidebar" aria-label="Arquivos alterados">
          {computedList.map(({ diff, stats }) => {
            const isSel = diff.id === activeEntry?.diff.id;
            return (
              <button
                key={diff.id}
                type="button"
                className={isSel ? "diff-file-item active" : "diff-file-item"}
                onClick={() => setSelectedId(diff.id)}
              >
                <div className="diff-file-top">
                  {diff.existedBefore ? <FileCode2 /> : <FilePlus2 />}
                  <strong>{diff.fileName}</strong>
                  {diff.reverted && <span className="diff-reverted-tag">Revertido</span>}
                </div>
                <span className="diff-file-path" title={diff.path}>
                  {diff.path}
                </span>
                <div className="diff-file-stats">
                  <span className="diff-plus">+{stats.added}</span>
                  <span className="diff-minus">-{stats.removed}</span>
                  <small>{diff.existedBefore ? "Editado" : "Criado"}</small>
                </div>
              </button>
            );
          })}
        </aside>

        {activeEntry && (
          <div className="diff-viewer">
            <div className="diff-viewer-bar">
              <div className="diff-viewer-meta">
                <strong>{activeEntry.diff.fileName}</strong>
                <code title={activeEntry.diff.path}>{activeEntry.diff.path}</code>
              </div>
              <div className="diff-viewer-actions">
                <button
                  type="button"
                  className="mini-btn"
                  onClick={() => void handleCopy()}
                >
                  {copied ? <Check /> : <Copy />} {copied ? "Copiado" : "Copiar diff"}
                </button>
                <button
                  type="button"
                  className="mini-btn accent"
                  disabled={activeEntry.diff.reverted || revertingId === activeEntry.diff.id}
                  onClick={() => void handleRevertClick(activeEntry.diff)}
                >
                  <RotateCcw />
                  {activeEntry.diff.reverted
                    ? "Revertido"
                    : revertingId === activeEntry.diff.id
                      ? "Revertendo..."
                      : activeEntry.diff.existedBefore
                        ? "Desfazer alteração"
                        : "Remover arquivo criado"}
                </button>
              </div>
            </div>

            <div className="diff-code" role="region" aria-label="Diff linha a linha">
              {activeEntry.stats.lines.length === 0 ? (
                <p className="empty-hint">Nenhuma diferença textual detectada neste arquivo.</p>
              ) : (
                activeEntry.stats.lines.map((line, idx) => (
                  <div key={idx} className={`diff-line ${line.kind}`}>
                    <span className="diff-ln">{line.oldLine ?? ""}</span>
                    <span className="diff-ln">{line.newLine ?? ""}</span>
                    <span className="diff-sig">
                      {line.kind === "add" ? "+" : line.kind === "del" ? "-" : " "}
                    </span>
                    <pre>{line.text || " "}</pre>
                  </div>
                ))
              )}
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
