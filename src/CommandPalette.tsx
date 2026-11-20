import { useEffect, useMemo, useRef, useState } from "react";
import {
  Bot,
  ClipboardPaste,
  Command,
  Eye,
  Globe,
  Maximize2,
  Palette,
  Plus,
  Radio,
  Search,
  SlidersHorizontal,
  Volume2,
  X,
} from "lucide-react";
import type { VisualTheme } from "./Stage";

export interface PaletteCommand {
  id: string;
  label: string;
  category: string;
  shortcut?: string;
  icon: "eye" | "clip" | "wake" | "mute" | "plus" | "globe" | "bot" | "theme" | "fs" | "settings";
  run: () => void;
}

function renderIcon(kind: PaletteCommand["icon"]) {
  switch (kind) {
    case "eye":
      return <Eye />;
    case "clip":
      return <ClipboardPaste />;
    case "wake":
      return <Radio />;
    case "mute":
      return <Volume2 />;
    case "plus":
      return <Plus />;
    case "globe":
      return <Globe />;
    case "bot":
      return <Bot />;
    case "theme":
      return <Palette />;
    case "fs":
      return <Maximize2 />;
    case "settings":
      return <SlidersHorizontal />;
  }
}

export function CommandPalette({
  open,
  wakeEnabled,
  muted,
  onClose,
  onRunPrompt,
  onPasteClipboard,
  onToggleWake,
  onToggleMute,
  onNewChat,
  onThemeChange,
  onToggleFullscreen,
  onOpenSettings,
}: {
  open: boolean;
  wakeEnabled: boolean;
  muted: boolean;
  onClose: () => void;
  onRunPrompt: (text: string) => void;
  onPasteClipboard: () => void;
  onToggleWake: () => void;
  onToggleMute: () => void;
  onNewChat: () => void;
  onThemeChange: (t: VisualTheme) => void;
  onToggleFullscreen: () => void;
  onOpenSettings: () => void;
}) {
  const [query, setQuery] = useState("");
  const [selectedIdx, setSelectedIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setQuery("");
      setSelectedIdx(0);
      window.setTimeout(() => inputRef.current?.focus(), 20);
    }
  }, [open]);

  const commands: PaletteCommand[] = useMemo(
    () => [
      {
        id: "see-game",
        label: "Olhar meu jogo ou janela aberta agora",
        category: "Visão Win32",
        shortcut: "PrintWindow / DXGI",
        icon: "eye",
        run: () =>
          onRunPrompt(
            "Liste minhas janelas abertas, identifique o que estou fazendo e capture a janela principal para analisar.",
          ),
      },
      {
        id: "read-clipboard",
        label: "Anexar texto ou print do meu Ctrl+C",
        category: "Área de Transferência",
        shortcut: "Ctrl+V",
        icon: "clip",
        run: onPasteClipboard,
      },
      {
        id: "toggle-wake",
        label: wakeEnabled
          ? 'Desativar Wake Word ("Ei, Jarvis")'
          : 'Ativar Wake Word ("Ei, Jarvis")',
        category: "Voz Nativa Rust",
        icon: "wake",
        run: onToggleWake,
      },
      {
        id: "new-chat",
        label: "Iniciar nova conversa limpa",
        category: "Conversas",
        icon: "plus",
        run: onNewChat,
      },
      {
        id: "open-browser",
        label: "Abrir navegador embutido 60fps",
        category: "Navegador CDP",
        icon: "globe",
        run: () => onRunPrompt("Abra o navegador embutido no Google."),
      },
      {
        id: "spawn-agent",
        label: "Criar subagente paralelo de pesquisa",
        category: "Agentes",
        icon: "bot",
        run: () =>
          onRunPrompt(
            "Crie um agente pesquisador em segundo plano para resumir as principais novidades de IA hoje.",
          ),
      },
      {
        id: "toggle-mute",
        label: muted ? "Reativar áudio e voz ElevenLabs" : "Silenciar áudio do J.A.R.V.I.S.",
        category: "Áudio",
        icon: "mute",
        run: onToggleMute,
      },
      {
        id: "theme-cyan",
        label: "Tema Visual: Stark Cyan (Padrão)",
        category: "Aparência 3D",
        icon: "theme",
        run: () => onThemeChange("cyan"),
      },
      {
        id: "theme-gold",
        label: "Tema Visual: Arc Gold",
        category: "Aparência 3D",
        icon: "theme",
        run: () => onThemeChange("gold"),
      },
      {
        id: "theme-violet",
        label: "Tema Visual: Quantum Violet",
        category: "Aparência 3D",
        icon: "theme",
        run: () => onThemeChange("violet"),
      },
      {
        id: "theme-crimson",
        label: "Tema Visual: Crimson Core",
        category: "Aparência 3D",
        icon: "theme",
        run: () => onThemeChange("crimson"),
      },
      {
        id: "fullscreen",
        label: "Alternar Tela Cheia",
        category: "Janela",
        shortcut: "F11",
        icon: "fs",
        run: onToggleFullscreen,
      },
      {
        id: "settings",
        label: "Abrir Ajustes de IA, Voz e Sistema",
        category: "Configuração",
        icon: "settings",
        run: onOpenSettings,
      },
    ],
    [
      wakeEnabled,
      muted,
      onRunPrompt,
      onPasteClipboard,
      onToggleWake,
      onNewChat,
      onToggleMute,
      onThemeChange,
      onToggleFullscreen,
      onOpenSettings,
    ],
  );

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    return commands.filter(
      (c) =>
        c.label.toLowerCase().includes(q) || c.category.toLowerCase().includes(q),
    );
  }, [commands, query]);

  if (!open) return null;

  function executeCommand(cmd: PaletteCommand | undefined) {
    if (!cmd) return;
    onClose();
    cmd.run();
  }

  return (
    <div
      className="cmd-palette-backdrop"
      role="dialog"
      aria-label="Paleta de comandos rápidos"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="cmd-palette-modal">
        <div className="cmd-palette-search">
          <Search />
          <input
            ref={inputRef}
            type="text"
            value={query}
            placeholder="Digite um comando ou ação do J.A.R.V.I.S. (Ctrl+K)…"
            aria-label="Buscar comando rápido"
            onChange={(e) => {
              setQuery(e.target.value);
              setSelectedIdx(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setSelectedIdx((i) => (filtered.length ? (i + 1) % filtered.length : 0));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setSelectedIdx((i) =>
                  filtered.length ? (i - 1 + filtered.length) % filtered.length : 0,
                );
              } else if (e.key === "Enter") {
                e.preventDefault();
                if (filtered[selectedIdx]) {
                  executeCommand(filtered[selectedIdx]);
                } else if (query.trim()) {
                  const text = query.trim();
                  onClose();
                  onRunPrompt(text);
                }
              } else if (e.key === "Escape") {
                e.preventDefault();
                onClose();
              }
            }}
          />
          <button
            type="button"
            className="icon-tiny"
            aria-label="Fechar paleta de comandos"
            onClick={onClose}
          >
            <X />
          </button>
        </div>

        <div className="cmd-palette-list" role="listbox">
          {filtered.length === 0 ? (
            <button
              type="button"
              className="cmd-palette-item active"
              onClick={() => {
                const text = query.trim();
                onClose();
                if (text) onRunPrompt(text);
              }}
            >
              <Command />
              <div className="cmd-item-text">
                <strong>Perguntar ao J.A.R.V.I.S.: &ldquo;{query.trim()}&rdquo;</strong>
                <small>Pressione Enter para enviar diretamente</small>
              </div>
            </button>
          ) : (
            filtered.map((cmd, idx) => (
              <button
                key={cmd.id}
                type="button"
                role="option"
                aria-selected={idx === selectedIdx}
                className={
                  idx === selectedIdx ? "cmd-palette-item active" : "cmd-palette-item"
                }
                onMouseEnter={() => setSelectedIdx(idx)}
                onClick={() => executeCommand(cmd)}
              >
                {renderIcon(cmd.icon)}
                <div className="cmd-item-text">
                  <strong>{cmd.label}</strong>
                  <small>{cmd.category}</small>
                </div>
                {cmd.shortcut && <kbd>{cmd.shortcut}</kbd>}
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
