import { useEffect, useState } from "react";
import {
  Bot,
  Check,
  Code2,
  Compass,
  Copy,
  Globe,
  Play,
  Send,
  Square,
  Terminal,
  Trash2,
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
  subtitle: string;
  sample: string;
}[] = [
  {
    role: "research",
    name: "Pesquisador Web",
    subtitle: "Pesquisa fontes atuais e entrega relatório estruturado",
    sample: "Pesquise as novidades mais importantes de IA desta semana e resuma em tópicos.",
  },
  {
    role: "coder",
    name: "Engenheiro de Código",
    subtitle: "Escreve, revisa e depura scripts e arquiteturas",
    sample: "Crie um script PowerShell para organizar minha pasta Downloads por tipo de arquivo.",
  },
  {
    role: "operator",
    name: "Operador Windows",
    subtitle: "Executa ações locais e automações no seu computador",
    sample: "Liste os arquivos recentes da área de trabalho e sugira como organizá-los.",
  },
  {
    role: "planner",
    name: "Estrategista",
    subtitle: "Transforma metas complexas em planos passo a passo",
    sample: "Monte um plano de estudos enxuto de 7 dias para dominar TypeScript + Rust.",
  },
];

function RoleIcon({ role }: { role: AgentRole }) {
  switch (role) {
    case "research":
      return <Globe />;
    case "coder":
      return <Code2 />;
    case "operator":
      return <Terminal />;
    case "planner":
      return <Compass />;
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
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());

  const hasRunning = agents.some((a) => a.status === "running");

  useEffect(() => {
    if (!hasRunning) return;
    const id = window.setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(id);
  }, [hasRunning]);

  const activePreset =
    AGENT_PRESETS.find((p) => p.role === selectedRole) ?? AGENT_PRESETS[0];

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
      window.setTimeout(() => setCopiedId((cur) => (cur === agent.id ? null : cur)), 1600);
    } catch {
      /* ignore */
    }
  }

  const runningCount = agents.filter((a) => a.status === "running").length;

  return (
    <section className="module-shell agents-shell" aria-label="Central de Agentes">
      <div className="module-header">
        <div className="module-title-group">
          <Bot className="module-icon" />
          <div>
            <h2>Central de Agentes</h2>
            <p>
              Execute vários subagentes em paralelo com busca na web e ferramentas do Windows.
            </p>
          </div>
        </div>
        <div className="module-stats">
          <span className={runningCount > 0 ? "hud-badge live" : "hud-badge"}>
            <i />
            {runningCount > 0
              ? `${runningCount} em execução`
              : `${agents.length} ${agents.length === 1 ? "missão" : "missões"}`}
          </span>
        </div>
      </div>

      <div className="agent-launcher-card">
        <div className="agent-preset-grid" role="radiogroup" aria-label="Especialidade do agente">
          {AGENT_PRESETS.map((preset) => (
            <button
              key={preset.role}
              type="button"
              role="radio"
              aria-checked={selectedRole === preset.role}
              className={
                selectedRole === preset.role
                  ? "agent-preset active"
                  : "agent-preset"
              }
              onClick={() => setSelectedRole(preset.role)}
            >
              <span className="agent-preset-icon">
                <RoleIcon role={preset.role} />
              </span>
              <span className="agent-preset-meta">
                <strong>{preset.name}</strong>
                <small>{preset.subtitle}</small>
              </span>
            </button>
          ))}
        </div>

        <form className="agent-launch-form" onSubmit={(e) => void handleSubmit(e)}>
          <input
            type="text"
            value={taskInput}
            placeholder={`Missão para ${activePreset.name}...`}
            aria-label="Missão do agente"
            onChange={(e) => setTaskInput(e.target.value)}
          />
          <button
            type="button"
            className="mini-btn"
            onClick={() => setTaskInput(activePreset.sample)}
            title="Preencher exemplo"
          >
            Exemplo
          </button>
          <button
            type="submit"
            className="mini-btn accent"
            disabled={!taskInput.trim() || launching}
          >
            <Play />
            Lançar Agente
          </button>
        </form>
      </div>

      {agents.length === 0 ? (
        <div className="module-empty">
          <Bot />
          <strong>Nenhum agente ativo no momento</strong>
          <p>
            Escolha uma especialidade acima e descreva uma missão — você pode rodar múltiplos
            agentes ao mesmo tempo enquanto continua usando o J.A.R.V.I.S.
          </p>
        </div>
      ) : (
        <div className="agent-cards-grid">
          {agents.map((agent) => (
            <article key={agent.id} className={`agent-card status-${agent.status}`}>
              <div className="agent-card-head">
                <div className="agent-card-title">
                  <span className="agent-preset-icon small">
                    <RoleIcon role={agent.role} />
                  </span>
                  <div>
                    <strong>{agent.name}</strong>
                    <span className="agent-status-line">
                      <i className={agent.status === "running" ? "live" : ""} />
                      {agent.status === "running"
                        ? "Executando"
                        : agent.status === "done"
                          ? "Concluído"
                          : agent.status === "cancelled"
                            ? "Cancelado"
                            : "Erro"}
                      {" · "}
                      {formatElapsed(agent.startedAt, agent.finishedAt, now)}
                    </span>
                  </div>
                </div>

                <div className="agent-card-actions">
                  {agent.status === "running" ? (
                    <button
                      type="button"
                      className="icon-tiny danger"
                      title="Parar agente"
                      aria-label="Parar agente"
                      onClick={() => void onCancel(agent.id)}
                    >
                      <Square />
                    </button>
                  ) : (
                    <>
                      {agent.output && (
                        <>
                          <button
                            type="button"
                            className="icon-tiny"
                            title="Enviar resultado para o Núcleo"
                            aria-label="Enviar resultado para o Núcleo"
                            onClick={() => onSendToCore(agent)}
                          >
                            <Send />
                          </button>
                          <button
                            type="button"
                            className="icon-tiny"
                            title="Copiar resultado"
                            aria-label="Copiar resultado"
                            onClick={() => void handleCopy(agent)}
                          >
                            {copiedId === agent.id ? <Check /> : <Copy />}
                          </button>
                        </>
                      )}
                      <button
                        type="button"
                        className="icon-tiny"
                        title="Remover card"
                        aria-label="Remover agente"
                        onClick={() => onRemove(agent.id)}
                      >
                        <Trash2 />
                      </button>
                    </>
                  )}
                </div>
              </div>

              <div className="agent-card-task">{agent.task}</div>

              {agent.traces.length > 0 && (
                <div className="tool-chips">
                  {agent.traces.map((t) => (
                    <span key={t.id} className={`tool-chip ${t.status}`}>
                      {t.title}
                    </span>
                  ))}
                </div>
              )}

              <div className="agent-card-body">
                {agent.error ? (
                  <p className="agent-error-text">{agent.error}</p>
                ) : agent.output ? (
                  <MarkdownBlocks text={agent.output} />
                ) : (
                  <p className="agent-thinking-text">
                    Inicializando contexto e analisando missão...
                  </p>
                )}
              </div>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}
