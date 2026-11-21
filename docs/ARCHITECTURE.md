# Arquitetura de Sistema — J.A.R.V.I.S. V8.0 (The Singularity Edition)

Este documento detalha os subsistemas nativos em **Rust (Win32)** e **React 19 + Three.js (WebGL2)** que compõem o **J.A.R.V.I.S. V8.0**.

---

## 1. Visão Geral Multi-Janela (Tauri 2)

```mermaid
flowchart LR
  subgraph Windows["Windows 10 / 11 Desktop"]
    Main["Janela Principal (main)\nNúcleo 3D + Chat + Drawers"]
    Mascot["Mascote Global (?window=mascot)\nTransparente · Always-On-Top · 60fps"]
    Browser["Child WebView2 (browser)\n60fps GPU Embedded · CDP"]
  end

  subgraph RustCore["Núcleo Rust (src-tauri)"]
    LLM["Orquestrador IA (llm.rs)\nChatGPT OAuth + Gemini"]
    Mic["Motor de Áudio (mic.rs)\nWin32 waveIn 16kHz + VAD"]
    Tools["Motor Win32 (tools.rs)\nPrintWindow + DXGI + SendInput + Clipboard"]
    TTS["Motor de Fala (tts.rs)\nElevenLabs Flash v2.5 + Cache SHA-256"]
  end

  Main <-->|IPC & Events| RustCore
  Mascot <-->|sync_mascot_state / mascot_action| RustCore
  Browser <-->|Chrome DevTools Protocol| RustCore
```

---

## 2. Pipeline de Visão Inteligente de Jogos & Janelas (`see_screen`)

```mermaid
sequenceDiagram
  participant U as Usuário
  participant LLM as J.A.R.V.I.S. (llm.rs)
  participant Win as Win32 (tools.rs)
  participant Target as Jogo / Janela Alvo

  U->>LLM: "Jarvis, olha meu jogo" (enquanto está no Firefox)
  LLM->>Win: list_open_windows()
  Win-->>LLM: [firefox.exe] YouTube | [javaw.exe] Minecraft 1.21.4
  LLM->>Win: see_screen(target_window: "Minecraft")
  Win->>Win: SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)
  Win->>Target: PrintWindow(hwnd, mem_dc, PW_RENDERFULLCONTENT)
  alt Quadro válido em 2º plano (Sem roubar foco)
    Target-->>Win: Pixels BGRA
  else Jogo DirectX/Vulkan exclusivo ou minimizado
    Win->>Target: ShowWindow(SW_RESTORE) + SetForegroundWindow
    Win->>Target: DXGI Desktop Duplication (IDXGIOutputDuplication)
  end
  Win-->>LLM: Imagem 1280x720 JPEG Base64 anexada ao turno multimodal
```

---

## 3. Pipeline de Voz Nativa & Wake Word `"Ei, Jarvis"` (`mic.rs`)

```mermaid
flowchart TD
  A["Microfone Windows\n(Win32 waveInOpen 16kHz 16-bit Mono)"] --> B["Buffer Circular 80ms\n+ Pré-Roll de 480ms"]
  B --> C{"VAD Local em Rust\n(RMS > 0.010?)"}
  C -- "Não (Silêncio)" --> D["0 Bytes Enviados\n0 Créditos Gastos"]
  C -- "Sim (Fala Detectada)" --> E[" acumula até pausa de 620ms\nou limite de 3.6s "]
  E --> F["Codifica WAV 16kHz em Memória"]
  F --> G["Transcrição Cloud\n(ElevenLabs Scribe v2 / Gemini)"]
  G --> H{"Contém 'Ei, Jarvis'\nou 'Jarvis [comando]'?"}
  H -- "Sim" --> I["Dispara jarvis:wake-triggered\nno Cockpit e no Mascote"]
```
