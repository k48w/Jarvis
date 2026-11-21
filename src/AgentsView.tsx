/**
 * # Painel de Subagentes Autônomos (`src/AgentsView.tsx`)
 *
 * Exibe o painel *Mission Control* (master-detail) para lançar, acompanhar
 * e cancelar subagentes em background (Pesquisador, Programador, Automação).
 */
import { useEffect, useState } from "react";
import {
  Bot,
  Check,
  CheckCircle2,
  Clock3,
  Code2,
  Compass,
  Copy,
  Globe,
  Loader2,
  Play,
  Send,
  Sparkles,
  Square,
  Terminal,
  Trash2,
  XCircle,
} from "lucide-react";
import type { AgentRole, ToolTrace } from "./bridge";
import { MarkdownBlocks } from "./ResponseCard";

export interface AgentRun {
  id: string;
  role: AgentRole;
  name: string;
  task: string;
  status: "running" | "done" | "error" | "cancelled";
  output: string;
  error?: string;
  startedAt: number;
  finishedAt?: number;
  traces: ToolTrace[];
}

export const AGENT_PRESETS: {
  role: AgentRole;
  name: string;
  short: string;
  subtitle: string;
  sample: string;
}[] = [
  {
    role: "research",
    name: "Pesquisador Web",
    short: "Pesquisa",
    subtitle: "Cruza fontes atuais e entrega relatório limpo",
    sample:
      "Pesquise as novidades mais importantes de IA desta semana e resuma em tópicos objetivos.",
  },
  {
    role: "coder",
    name: "Engenheiro de Código",
    short: "Código",
    subtitle: "Escreve, revisa e depura scripts e arquiteturas",
    sample:
      "Crie um script PowerShell seguro para organizar minha pasta Downloads por tipo de arquivo.",
  },
  {
    role: "operator",
    name: "Operador Windows",
    short: "Windows",
    subtitle: "Executa ações locais e automações no sistema",
    sample:
      "Liste os arquivos recentes da área de trabalho e sugira como organizá-los.",
  },
  {
    role: "planner",
    name: "Estrategista",
    short: "Plano",
    subtitle: "Quebra objetivos complexos em etapas claras",
    sample:
      "Monte um plano prático de 7 dias para dominar TypeScript + Rust do zero.",
  },
];

function RoleIcon({ role, size = 15 }: { role: AgentRole; size?: number }) {
  switch (role) {
    case "research":
      return <Globe size={size} />;
    case "coder":
      return <Code2 size={size} />;
    case "operator":
      return <Terminal size={size} />;
    case "planner":
      return <Compass size={size} />;
  }
}

function StatusIcon({ status }: { status: AgentRun["status"] }) {
  switch (status) {
    case "running":
      return <Loader2 size={13} className="spin" />;
    case "done":
      return <CheckCircle2 size={13} />;
    case "error":
    case "cancelled":
      return <XCircle size={13} />;
  }
}

function statusLabel(status: AgentRun["status"]) {
  switch (status) {
    case "running":
      return "Executando";
    case "done":
      return "Concluído";
    case "cancelled":
      return "Cancelado";
    case "error":
      return "Erro";
  }
}

function formatElapsed(startMs: number, endMs?: number, nowMs = Date.now()) {
  const sec = Math.max(0, Math.floor(((endMs ?? nowMs) - startMs) / 1000));
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

export function AgentsView({
  agents,
  onLaunch,
  onCancel,
  onRemove,
  onSendToCore,
}: {
  agents: AgentRun[];
  onLaunch: (role: AgentRole, name: string, task: string) => Promise<void>;
  onCancel: (id: string) => Promise<void>;
  onRemove: (id: string) => void;
  onSendToCore: (agent: AgentRun) => void;
}) {
  const [selectedRole, setSelectedRole] = useState<AgentRole>("research");
  const [taskInput, setTaskInput] = useState("");
  const [launching, setLaunching] = useState(false);
  const [selectedId, setSelectedId] = useState<string>(agents[0]?.id ?? "");
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());

  const hasRunning = agents.some((a) => a.status === "running");

  useEffect(() => {
    if (!hasRunning) return;
    const id = window.setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(id);
  }, [hasRunning]);

  // Auto-select newest agent when a new one appears
  useEffect(() => {
    if (agents.length > 0 && !agents.some((a) => a.id === selectedId)) {
      setSelectedId(agents[0].id);
    }
  }, [agents, selectedId]);

  const activePreset =
    AGENT_PRESETS.find((p) => p.role === selectedRole) ?? AGENT_PRESETS[0];
  const activeAgent =
    agents.find((a) => a.id === selectedId) ?? agents[0] ?? null;

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const clean = taskInput.trim();
    if (!clean || launching) return;
    setLaunching(true);
    try {
      await onLaunch(selectedRole, activePreset.name, clean);
      setTaskInput("");
    } finally {
      setLaunching(false);
    }
  }

  async function handleCopy(agent: AgentRun) {
    try {
      await navigator.clipboard.writeText(agent.output);
      setCopiedId(agent.id);
      window.setTimeout(
        () => setCopiedId((cur) => (cur === agent.id ? null : cur)),
        1600,
      );
    } catch {
      /* ignore */
    }
  }

  const runningCount = agents.filter((a) => a.status === "running").length;
  const doneCount = agents.filter((a) => a.status === "done").length;

  return (
    <section
      className="ag-shell"
      aria-label="Central de Agentes"
    >
      {/* Top Mission Control Bar */}
      <header className="ag-topbar">
        <div className="ag-brand">
          <span className="ag-brand-icon">
            <Bot size={16} />
          </span>
          <div>
            <h2>Agentes Autônomos</h2>
            <p>Até 3 missões paralelas em segundo plano com resumo falado</p>
          </div>
        </div>

        <div className="ag-metrics">
          <span className={runningCount > 0 ? "ag-pill live" : "ag-pill"}>
            <i />
            {runningCount > 0 ? `${runningCount} rodando` : "Em espera"}
          </span>
          {doneCount > 0 && (
            <span className="ag-pill done">{doneCount} concluído(s)</span>
          )}
        </div>
      </header>

      {/* Sleek Compact Command Launcher */}
      <div className="ag-composer">
        <div
          className="ag-role-pills"
          role="radiogroup"
          aria-label="Especialidade do agente"
        >
          {AGENT_PRESETS.map((preset) => (
            <button
              key={preset.role}
              type="button"
              role="radio"
              aria-checked={selectedRole === preset.role}
              className={
                selectedRole === preset.role
                  ? `ag-role-pill role-${preset.role} active`
                  : `ag-role-pill role-${preset.role}`
              }
              onClick={() => setSelectedRole(preset.role)}
              title={preset.subtitle}
            >
              <RoleIcon role={preset.role} size={13} />
              <span>{preset.short}</span>
            </button>
          ))}
        </div>

        <form className="ag-form" onSubmit={(e) => void handleSubmit(e)}>
          <input
            type="text"
            value={taskInput}
            placeholder={`Delegar missão para ${activePreset.name}...`}
            aria-label="Missão do agente"
            onChange={(e) => setTaskInput(e.target.value)}
          />
          <button
            type="button"
            className="ag-btn ghost"
            onClick={() => setTaskInput(activePreset.sample)}
            title="Preencher exemplo"
          >
            <Sparkles size={13} />
            Exemplo
          </button>
          <button
            type="submit"
            className="ag-btn primary"
            disabled={!taskInput.trim() || launching}
          >
            <Play size={13} />
            Lançar
          </button>
        </form>
      </div>

      {/* Main Content: Empty State or Master-Detail Split */}
      {agents.length === 0 ? (
        <div className="ag-empty">
          <div className="ag-empty-orb">
            <Bot size={26} />
          </div>
          <h3>Nenhuma missão ativa</h3>
          <p>
            Escolha uma especialidade acima ou peça por voz ao J.A.R.V.I.S. — quando
            concluir, ele fala o resumo automaticamente.
          </p>
        </div>
      ) : (
        <div className="ag-workspace">
          {/* Left Rail: Mission List */}
          <aside className="ag-rail" aria-label="Lista de agentes">
            {agents.map((agent) => {
              const isSelected = agent.id === activeAgent?.id;
              return (
                <button
                  key={agent.id}
                  type="button"
                  className={`ag-rail-item status-${agent.status} ${isSelected ? "selected" : ""}`}
                  onClick={() => setSelectedId(agent.id)}
                >
                  <div className="ag-rail-top">
                    <span className={`ag-role-dot role-${agent.role}`}>
                      <RoleIcon role={agent.role} size={13} />
                    </span>
                    <strong className="ag-rail-name">{agent.name}</strong>
                    <span className={`ag-status-badge status-${agent.status}`}>
                      <StatusIcon status={agent.status} />
                    </span>
                  </div>
                  <p className="ag-rail-task">{agent.task}</p>
                  <div className="ag-rail-foot">
                    <span>
                      <Clock3 size={11} />{" "}
                      {formatElapsed(agent.startedAt, agent.finishedAt, now)}
                    </span>
                    <span>
                      {agent.traces.length > 0
                        ? `${agent.traces.length} ação(ões)`
                        : statusLabel(agent.status)}
                    </span>
                  </div>
                </button>
              );
            })}
          </aside>

          {/* Right Stage: Selected Agent Detail */}
          {activeAgent && (
            <article
              className={`ag-detail status-${activeAgent.status}`}
              aria-label={`Detalhes de ${activeAgent.name}`}
            >
              <div className="ag-detail-head">
                <div className="ag-detail-title">
                  <span className={`ag-role-dot lg role-${activeAgent.role}`}>
                    <RoleIcon role={activeAgent.role} size={16} />
                  </span>
                  <div>
                    <div className="ag-detail-name-row">
                      <h3>{activeAgent.name}</h3>
                      <span
                        className={`ag-status-pill status-${activeAgent.status}`}
                      >
                        <StatusIcon status={activeAgent.status} />
                        {statusLabel(activeAgent.status)}
                      </span>
                    </div>
                    <span className="ag-detail-time">
                      Tempo decorrido:{" "}
                      <code>
                        {formatElapsed(
                          activeAgent.startedAt,
                          activeAgent.finishedAt,
                          now,
                        )}
                      </code>
                    </span>
                  </div>
                </div>

                <div className="ag-detail-actions">
                  {activeAgent.status === "running" ? (
                    <button
                      type="button"
                      className="ag-btn danger"
                      onClick={() => void onCancel(activeAgent.id)}
                    >
                      <Square size={12} /> Parar
                    </button>
                  ) : (
                    <>
                      {activeAgent.output && (
                        <>
                          <button
                            type="button"
                            className="ag-btn ghost"
                            onClick={() => onSendToCore(activeAgent)}
                            title="Enviar relatório para a conversa principal"
                          >
                            <Send size={12} /> Enviar ao Núcleo
                          </button>
                          <button
                            type="button"
                            className="ag-btn ghost"
                            onClick={() => void handleCopy(activeAgent)}
                          >
                            {copiedId === activeAgent.id ? (
                              <Check size={12} />
                            ) : (
                              <Copy size={12} />
                            )}
                            {copiedId === activeAgent.id ? "Copiado" : "Copiar"}
                          </button>
                        </>
                      )}
                      <button
                        type="button"
                        className="ag-btn icon-only"
                        title="Remover missão"
                        aria-label="Remover missão"
                        onClick={() => onRemove(activeAgent.id)}
                      >
                        <Trash2 size={13} />
                      </button>
                    </>
                  )}
                </div>
              </div>

              <div className="ag-mission-box">
                <span className="ag-mission-label">MISSÃO</span>
                <p>{activeAgent.task}</p>
              </div>

              {activeAgent.traces.length > 0 && (
                <div className="ag-traces" aria-label="Etapas executadas">
                  {activeAgent.traces.map((t) => (
                    <span key={t.id} className={`ag-trace-chip ${t.status}`}>
                      <i />
                      {t.title}
                    </span>
                  ))}
                </div>
              )}

              <div className="ag-output-scroll">
                {activeAgent.error ? (
                  <div className="ag-error-box">{activeAgent.error}</div>
                ) : activeAgent.output ? (
                  <MarkdownBlocks text={activeAgent.output} />
                ) : (
                  <div className="ag-loading-state">
                    <Loader2 size={18} className="spin" />
                    <span>Pesquisando e estruturando resposta...</span>
                  </div>
                )}
              </div>
            </article>
          )}
        </div>
      )}
    </section>
  );
}
