import { useEffect, useRef, type RefObject } from "react";
import type { CoreState } from "./rhythm";

const TICK_MARKS = Array.from({ length: 72 }, (_, i) => i * 5);

export function HudOverlay({
  state,
  immersive,
}: {
  state: CoreState;
  immersive: boolean;
}) {
  return (
    <div className="hud-layer" data-state={state} aria-hidden="true">
      <div className="hud-vignette" />
      <div className="hud-scanlines" />

      {!immersive && (
        <>
          <div className="hud-corner tl" />
          <div className="hud-corner tr" />
          <div className="hud-corner bl" />
          <div className="hud-corner br" />
        </>
      )}

      <svg
        className="hud-reticle"
        viewBox="0 0 600 600"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
      >
        <g className="hud-ring-outer">
          <circle
            cx="300"
            cy="300"
            r="268"
            stroke="currentColor"
            strokeOpacity="0.26"
            strokeWidth="1.1"
            strokeDasharray="4 10"
          />
          <path
            d="M 88 176 A 244 244 0 0 1 200 78"
            stroke="currentColor"
            strokeOpacity="0.62"
            strokeWidth="2"
            strokeLinecap="round"
          />
          <path
            d="M 512 424 A 244 244 0 0 1 400 522"
            stroke="currentColor"
            strokeOpacity="0.62"
            strokeWidth="2"
            strokeLinecap="round"
          />
        </g>

        <g className="hud-ring-ticks">
          {TICK_MARKS.map((deg) => {
            const major = deg % 30 === 0;
            const r1 = major ? 245 : 252;
            const r2 = 259;
            const rad = (deg * Math.PI) / 180;
            return (
              <line
                key={deg}
                x1={300 + Math.cos(rad) * r1}
                y1={300 + Math.sin(rad) * r1}
                x2={300 + Math.cos(rad) * r2}
                y2={300 + Math.sin(rad) * r2}
                stroke="currentColor"
                strokeOpacity={major ? 0.62 : 0.25}
                strokeWidth={major ? 1.6 : 1}
              />
            );
          })}
        </g>

        <g className="hud-ring-inner">
          <circle
            cx="300"
            cy="300"
            r="214"
            stroke="currentColor"
            strokeOpacity="0.34"
            strokeWidth="1.1"
            strokeDasharray="44 18 8 18"
          />
          <circle
            cx="300"
            cy="300"
            r="192"
            stroke="currentColor"
            strokeOpacity="0.18"
            strokeWidth="1"
            strokeDasharray="2 12"
          />
        </g>

        <line
          x1="14"
          y1="300"
          x2="48"
          y2="300"
          stroke="currentColor"
          strokeOpacity="0.48"
          strokeWidth="1.2"
        />
        <line
          x1="552"
          y1="300"
          x2="586"
          y2="300"
          stroke="currentColor"
          strokeOpacity="0.48"
          strokeWidth="1.2"
        />
      </svg>
    </div>
  );
}

export function VoiceSpectrum({
  state,
  audioLevel,
  active,
}: {
  state: CoreState;
  audioLevel: RefObject<number>;
  active: boolean;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let raf = 0;
    const bars = 33;
    const smoothed = new Float32Array(bars);

    const draw = (now: number) => {
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const cssW = 300;
      const cssH = 26;
      if (canvas.width !== cssW * dpr || canvas.height !== cssH * dpr) {
        canvas.width = cssW * dpr;
        canvas.height = cssH * dpr;
      }
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, cssW, cssH);

      const accent =
        getComputedStyle(canvas).getPropertyValue("--accent").trim() || "#6ce8ff";
      const t = now * 0.004;
      const lvl = Math.min(1, Math.max(0, audioLevel.current ?? 0));
      const mid = (bars - 1) / 2;
      const gap = 4;
      const barW = (cssW - (bars - 1) * gap) / bars;

      for (let i = 0; i < bars; i++) {
        const dist = Math.abs(i - mid) / mid;
        const arch = Math.pow(1 - dist * 0.88, 1.5);
        let target = 0.08;
        if (state === "listening" || state === "responding") {
          const wave =
            0.55 + 0.45 * Math.sin(t * 2.1 + i * 0.65) * Math.cos(t * 1.15 - i * 0.35);
          target = 0.1 + arch * (0.14 + lvl * 0.95) * wave;
        } else if (state === "thinking") {
          const sweep = 0.5 + 0.5 * Math.sin(t * 2.4 - i * 0.42);
          target = 0.1 + arch * 0.46 * sweep;
        } else if (state === "alert") {
          const pulse = 0.5 + 0.5 * Math.sin(t * 3.4);
          target = 0.12 + arch * 0.55 * pulse;
        } else {
          const breathe = 0.5 + 0.5 * Math.sin(t * 0.65 + i * 0.25);
          target = 0.07 + arch * 0.14 * breathe;
        }

        smoothed[i] += (target - smoothed[i]) * 0.24;
        const h = Math.max(2, smoothed[i] * cssH);
        const x = i * (barW + gap);
        const y = (cssH - h) / 2;

        ctx.fillStyle = accent;
        ctx.globalAlpha = (active ? 0.42 : 0.22) + smoothed[i] * 0.58;
        ctx.beginPath();
        ctx.roundRect(x, y, barW, h, 2);
        ctx.fill();
      }
      ctx.globalAlpha = 1;

      raf = requestAnimationFrame(draw);
    };

    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, [state, audioLevel, active]);

  return (
    <canvas
      ref={canvasRef}
      className={active ? "voice-spectrum active" : "voice-spectrum"}
      aria-hidden="true"
    />
  );
}
