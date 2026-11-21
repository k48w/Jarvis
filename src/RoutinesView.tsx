/**
 * # Painel de Skills (`/comandos`) & Rotinas Agendadas (`src/RoutinesView.tsx`)
 *
 * Permite criar e disparar macros rápidas (`/briefing`, `/saude-pc`, `/foco`, etc.)
 * e configurar rotinas recorrentes (diárias em `HH:MM` ou por intervalo em minutos).
 */
import { useState } from "react";
import {
  Clock,
  Play,
  Plus,
  Sparkles,
  Trash2,
  Zap,
} from "lucide-react";
import type { RoutineItem, SkillItem } from "./bridge";

export function RoutinesView({
  skills,
  routines,
  onRunPrompt,
  onSaveSkill,
  onDeleteSkill,
  onCreateRoutine,
  onToggleRoutine,
  onDeleteRoutine,
}: {
  skills: SkillItem[];
  routines: RoutineItem[];
  onRunPrompt: (prompt: string) => void;
  onSaveSkill: (
    slug: string,
    name: string,
    description: string,
    prompt: string,
  ) => Promise<void>;
  onDeleteSkill: (idOrSlug: string) => Promise<void>;
  onCreateRoutine: (
    name: string,
    scheduleKind: "interval" | "daily",
    intervalMinutes: number | undefined,
    dailyHhmm: string | undefined,
    prompt: string,
  ) => Promise<void>;
  onToggleRoutine: (id: string, enabled: boolean) => Promise<void>;
  onDeleteRoutine: (id: string) => Promise<void>;
}) {
  const [tab, setTab] = useState<"skills" | "routines">("skills");

  // Skill form
  const [slug, setSlug] = useState("");
  const [skillName, setSkillName] = useState("");
  const [skillDesc, setSkillDesc] = useState("");
  const [skillPrompt, setSkillPrompt] = useState("");

  // Routine form
  const [rName, setRName] = useState("");
  const [rKind, setRKind] = useState<"daily" | "interval">("daily");
  const [rTime, setRTime] = useState("09:00");
  const [rInterval, setRInterval] = useState(60);
  const [rPrompt, setRPrompt] = useState("");

  const [busy, setBusy] = useState(false);

  async function handleAddSkill(e: React.FormEvent) {
    e.preventDefault();
    if (!slug.trim() || !skillPrompt.trim() || busy) return;
    setBusy(true);
    try {
      await onSaveSkill(slug, skillName, skillDesc, skillPrompt);
      setSlug("");
      setSkillName("");
      setSkillDesc("");
      setSkillPrompt("");
    } finally {
      setBusy(false);
    }
  }

  async function handleAddRoutine(e: React.FormEvent) {
    e.preventDefault();
    if (!rName.trim() || !rPrompt.trim() || busy) return;
    setBusy(true);
    try {
      await onCreateRoutine(
        rName,
        rKind,
        rKind === "interval" ? rInterval : undefined,
        rKind === "daily" ? rTime : undefined,
        rPrompt,
      );
      setRName("");
      setRPrompt("");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="panel-shell routines-shell" aria-label="Skills e Rotinas Proativas">
      <div className="panel-header">
        <div>
          <h2>
            <Zap size={17} style={{ verticalAlign: "-3px", marginRight: 6 }} />
            Skills &amp; Rotinas Proativas
          </h2>
          <p>
            Execute macros com <code>/comando</code> no chat ou agende rotinas automáticas para o J.A.R.V.I.S. agir sozinho.
          </p>
        </div>
        <div className="routines-tabs">
          <button
            type="button"
            className={tab === "skills" ? "mini-btn accent" : "mini-btn"}
            onClick={() => setTab("skills")}
          >
            <Sparkles size={13} /> Skills ({skills.length})
          </button>
          <button
            type="button"
            className={tab === "routines" ? "mini-btn accent" : "mini-btn"}
            onClick={() => setTab("routines")}
          >
            <Clock size={13} /> Rotinas ({routines.length})
          </button>
        </div>
      </div>

      {tab === "skills" ? (
        <div className="routines-grid">
          <div className="routines-list">
            {skills.map((s) => (
              <div key={s.id} className="routine-card">
                <div className="routine-card-top">
                  <div>
                    <span className="skill-slug-pill">/{s.slug}</span>
                    <strong>{s.name}</strong>
                  </div>
                  <div className="routine-card-actions">
                    <button
                      type="button"
                      className="mini-btn accent"
                      onClick={() => onRunPrompt(s.prompt)}
                      title="Executar skill agora"
                    >
                      <Play size={12} /> Rodar
                    </button>
                    {!s.builtin && (
                      <button
                        type="button"
                        className="mini-btn"
                        onClick={() => void onDeleteSkill(s.id)}
                        title="Excluir skill"
                        aria-label={`Excluir skill ${s.name}`}
                      >
                        <Trash2 size={12} />
                      </button>
                    )}
                  </div>
                </div>
                {s.description && <p className="routine-desc">{s.description}</p>}
                <code className="routine-prompt-preview">{s.prompt}</code>
              </div>
            ))}
          </div>

          <form className="routine-form" onSubmit={(e) => void handleAddSkill(e)}>
            <h3>
              <Plus size={14} /> Nova Skill Customizada
            </h3>
            <input
              type="text"
              placeholder="Comando (ex: modo-jogo)"
              value={slug}
              onChange={(e) => setSlug(e.target.value)}
            />
            <input
              type="text"
              placeholder="Nome visível (ex: Modo Jogo)"
              value={skillName}
              onChange={(e) => setSkillName(e.target.value)}
            />
            <input
              type="text"
              placeholder="Descrição curta (opcional)"
              value={skillDesc}
              onChange={(e) => setSkillDesc(e.target.value)}
            />
            <textarea
              rows={3}
              placeholder="O que o J.A.R.V.I.S. deve fazer quando você digitar /comando..."
              value={skillPrompt}
              onChange={(e) => setSkillPrompt(e.target.value)}
            />
            <button
              type="submit"
              className="mini-btn accent"
              disabled={busy || !slug.trim() || !skillPrompt.trim()}
            >
              <Plus size={13} /> Salvar Skill
            </button>
          </form>
        </div>
      ) : (
        <div className="routines-grid">
          <div className="routines-list">
            {routines.length === 0 ? (
              <p className="empty-hint">
                Nenhuma rotina agendada ainda. Crie uma ao lado ou peça ao J.A.R.V.I.S.: &ldquo;cria uma rotina todo dia às 09:00 para me dar o briefing&rdquo;.
              </p>
            ) : (
              routines.map((r) => (
                <div key={r.id} className="routine-card">
                  <div className="routine-card-top">
                    <div>
                      <span className="skill-slug-pill">
                        {r.scheduleKind === "daily"
                          ? `Todo dia ${r.dailyHhmm ?? "09:00"}`
                          : `A cada ${r.intervalMinutes ?? 60} min`}
                      </span>
                      <strong>{r.name}</strong>
                    </div>
                    <div className="routine-card-actions">
                      <button
                        type="button"
                        className="mini-btn"
                        onClick={() => onRunPrompt(r.prompt)}
                        title="Executar agora"
                        aria-label={`Executar rotina ${r.name}`}
                      >
                        <Play size={12} />
                      </button>
                      <button
                        type="button"
                        className={r.enabled ? "mini-btn accent" : "mini-btn"}
                        onClick={() => void onToggleRoutine(r.id, !r.enabled)}
                      >
                        {r.enabled ? "Ativa" : "Pausada"}
                      </button>
                      <button
                        type="button"
                        className="mini-btn"
                        onClick={() => void onDeleteRoutine(r.id)}
                        title="Excluir rotina"
                        aria-label={`Excluir rotina ${r.name}`}
                      >
                        <Trash2 size={12} />
                      </button>
                    </div>
                  </div>
                  <code className="routine-prompt-preview">{r.prompt}</code>
                </div>
              ))
            )}
          </div>

          <form className="routine-form" onSubmit={(e) => void handleAddRoutine(e)}>
            <h3>
              <Plus size={14} /> Nova Rotina Agendada
            </h3>
            <input
              type="text"
              placeholder="Nome (ex: Briefing Matinal)"
              value={rName}
              onChange={(e) => setRName(e.target.value)}
            />
            <div className="routine-schedule-row">
              <select
                value={rKind}
                onChange={(e) => setRKind(e.target.value as "daily" | "interval")}
              >
                <option value="daily">Horário diário</option>
                <option value="interval">A cada X minutos</option>
              </select>
              {rKind === "daily" ? (
                <input
                  type="time"
                  value={rTime}
                  onChange={(e) => setRTime(e.target.value)}
                />
              ) : (
                <input
                  type="number"
                  min={1}
                  max={1440}
                  value={rInterval}
                  onChange={(e) => setRInterval(Number(e.target.value) || 60)}
                />
              )}
            </div>
            <textarea
              rows={3}
              placeholder="Instrução que o J.A.R.V.I.S. executará automaticamente..."
              value={rPrompt}
              onChange={(e) => setRPrompt(e.target.value)}
            />
            <button
              type="submit"
              className="mini-btn accent"
              disabled={busy || !rName.trim() || !rPrompt.trim()}
            >
              <Plus size={13} /> Criar Rotina
            </button>
          </form>
        </div>
      )}
    </section>
  );
}
