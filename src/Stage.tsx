import { Component, type ReactNode, useState, useEffect, useLayoutEffect, useMemo, useRef, type RefObject } from 'react';
import { cadence, contraction, overtone, respondingEnvelope, type CoreState } from './rhythm';
import {
  AdditiveBlending,
  Color,
  DoubleSide,
  Group,
  MathUtils,
  Vector3,
  Vector4,
  type Mesh,
  type MeshBasicMaterial,
  type ShaderMaterialParameters,
} from 'three';
import { Canvas, useFrame } from '@react-three/fiber';
import { atmosphere, filamentFragment, filamentVertex, shellVertex, surface, vertex } from './render/plasma';
import { fieldFragment, fieldVertex, fieldVertices, filamentRibbons } from './render/field';

export type VisualTheme = 'cyan' | 'gold' | 'violet' | 'crimson';

export const THEME_VECTORS: Record<VisualTheme, [number, number, number]> = {
  cyan: [0.35, 0.92, 1.0],
  gold: [1.0, 0.78, 0.28],
  violet: [0.68, 0.48, 1.0],
  crimson: [1.0, 0.28, 0.36],
};

const THEME_RING_HEX: Record<VisualTheme, string> = {
  cyan: '#74e8ff',
  gold: '#ffd06b',
  violet: '#b794ff',
  crimson: '#ff6477',
};

export interface StageProps {
  state: CoreState;
  quality: 'Low' | 'High' | 'Ultra';
  theme: VisualTheme;
  reduced: boolean;
  audioLevel?: RefObject<number>;
}

const settings = {
  Low: { grains: 3600, arcs: 12, dpr: 1 },
  High: { grains: 9200, arcs: 24, dpr: 1.5 },
  Ultra: { grains: 18000, arcs: 38, dpr: 2 },
};
const arcDefines = { ARC: 1 };

function CoreScene({
  state,
  quality,
  theme,
  reduced,
  responseStart,
  audioLevel,
  pointerRef,
}: StageProps & {
  responseStart: RefObject<number | null>;
  pointerRef: RefObject<{ x: number; y: number }>;
}) {
  const profile = settings[quality];
  const grains = useMemo(() => fieldVertices(profile.grains, false), [profile]);
  const arcs = useMemo(() => fieldVertices(profile.arcs * 128, true), [profile]);
  const ribbons = useMemo(filamentRibbons, []);
  const core = useRef<Group>(null);
  const heart = useRef<Mesh>(null);
  const gyroOuter = useRef<Mesh>(null);
  const gyroMid = useRef<Mesh>(null);
  const gyroInner = useRef<Mesh>(null);
  const gyroMat1 = useRef<MeshBasicMaterial>(null);
  const gyroMat2 = useRef<MeshBasicMaterial>(null);
  const gyroMat3 = useRef<MeshBasicMaterial>(null);
  const cueMix = useRef(0);
  const smoothLevel = useRef(0);
  const smoothPointer = useRef({ x: 0, y: 0 });
  const ringColor = useMemo(() => new Color(THEME_RING_HEX.cyan), []);
  const alertColor = useMemo(() => new Color('#ffb86c'), []);
  const listenColor = useMemo(() => new Color('#58ffe0'), []);
  const themeTargetVec = useMemo(() => new Vector3(...THEME_VECTORS[theme]), [theme]);

  const uniforms = useMemo(
    () => ({
      uFlow: { value: 0 },
      uCharge: { value: 0 },
      uImpact: { value: 0 },
      uPhase: { value: 0 },
      uMotion: { value: reduced ? 0.14 : 1 },
      uPixel: { value: 1 },
      uPulse: { value: 0 },
      uGlint: { value: 0 },
      uState: { value: new Vector4(0, 0, 0, 0) },
      uTheme: { value: new Vector3(...THEME_VECTORS[theme]) },
    }),
    [],
  );

  // Constructor parameters preserve shared uniform references across R3F 9 versions.
  const materialArgs = useMemo<[ShaderMaterialParameters]>(() => [{ uniforms }], [uniforms]);

  const target = useMemo(
    () =>
      new Vector4(
        Number(state === 'listening'),
        Number(state === 'thinking'),
        Number(state === 'responding'),
        Number(state === 'alert'),
      ),
    [state],
  );

  useFrame(({ camera, size, gl }, delta) => {
    // Suspend the performance while the WebView is asleep.
    const dt = Math.min(delta, 0.25);
    const blend = -Math.expm1(-2.2 * dt);
    const weights = uniforms.uState.value.lerp(target, blend);
    uniforms.uTheme.value.lerp(themeTargetVec, -Math.expm1(-3.5 * dt));

    const { x: listen, y: think, z: respond, w: alert } = weights;
    const idle = Math.max(0, 1 - listen - think - respond - alert);
    const frequency =
      idle * cadence.idle +
      listen * cadence.listening +
      think * cadence.thinking +
      respond * cadence.responding +
      alert * cadence.alert;
    const motion = MathUtils.lerp(uniforms.uMotion.value, reduced ? 0.14 : 1, blend);
    uniforms.uMotion.value = motion;
    const phase = (uniforms.uPhase.value + dt * frequency) % 1;
    uniforms.uPhase.value = phase;

    const rawLevel = Math.min(1, Math.max(0, audioLevel?.current ?? 0));
    const levelRate = rawLevel > smoothLevel.current ? 26 : 8;
    smoothLevel.current = MathUtils.lerp(smoothLevel.current, rawLevel, -Math.expm1(-levelRate * dt));
    const liveVoice = smoothLevel.current;

    const charge =
      0.18 +
      0.28 * listen +
      0.57 * think +
      0.66 * respond +
      0.82 * alert +
      liveVoice * (0.34 * listen + 0.24 * respond);
    const flow =
      uniforms.uFlow.value +
      dt * motion * (0.34 + 0.29 * listen + 1.12 * think + 0.6 * respond + 0.92 * alert);

    const responding = state === 'responding';
    const age = responseStart.current === null ? 0 : (performance.now() - responseStart.current) / 1000;
    const cue = respondingEnvelope(age);
    cueMix.current = responding
      ? 1 - Math.exp(-Math.max(0, age) / 0.024)
      : cueMix.current * Math.exp(-dt * 9);
    const baseBeat = MathUtils.lerp(contraction(phase) * (0.43 + 0.57 * charge), cue, cueMix.current);
    const voiceGate = Math.min(1, liveVoice * 6);
    const beat =
      MathUtils.lerp(baseBeat, Math.max(baseBeat * 0.35, liveVoice * 0.95), voiceGate * (listen + respond)) *
      motion;
    uniforms.uPulse.value = beat;

    const baseGlint = MathUtils.lerp(overtone(phase) * (0.22 + 0.78 * charge), cue, cueMix.current);
    uniforms.uGlint.value = Math.max(baseGlint, liveVoice * 0.85 * (listen + respond)) * motion;
    uniforms.uFlow.value = flow;
    uniforms.uCharge.value = charge;
    uniforms.uImpact.value = alert * (0.44 + 0.56 * beat) + 0.35 * respond * Math.max(beat, liveVoice);
    uniforms.uPixel.value = (size.height * gl.getPixelRatio()) / 92;

    // Smooth interactive parallax from pointer position
    const ptr = pointerRef.current ?? { x: 0, y: 0 };
    const ptrBlend = -Math.expm1(-4.2 * dt);
    smoothPointer.current.x = MathUtils.lerp(smoothPointer.current.x, ptr.x * motion, ptrBlend);
    smoothPointer.current.y = MathUtils.lerp(smoothPointer.current.y, ptr.y * motion, ptrBlend);

    const aspect = size.width / Math.max(1, size.height);
    const distance =
      (6.45 - motion * (0.24 * listen + 0.18 * respond)) / Math.min(1, Math.max(0.15, aspect));
    const orbit = motion * 0.09 * Math.sin(flow * 0.065) + smoothPointer.current.x * 0.28;
    const elev = 0.1 * motion - smoothPointer.current.y * 0.42;
    camera.position.set(distance * Math.sin(orbit), elev, distance * Math.cos(orbit));
    camera.lookAt(0, 0, 0);

    if (heart.current) {
      heart.current.rotation.y = -flow * 0.11;
      heart.current.scale.setScalar(0.56 + 0.065 * beat);
    }
    if (core.current) {
      core.current.rotation.set(0.12 + smoothPointer.current.y * 0.14, flow * 0.042, -0.32);
      core.current.scale.setScalar(0.97 + 0.034 * respond - 0.075 * beat);
    }

    // Gyroscopic holographic rings reacting to state & live voice
    const ringSpeed = 1 + think * 2.2 + respond * 0.9 + alert * 1.8;
    if (gyroOuter.current) {
      gyroOuter.current.rotation.set(
        1.18 + Math.sin(flow * 0.16) * 0.12,
        flow * 0.16 * ringSpeed,
        0.35,
      );
      gyroOuter.current.scale.setScalar(1.0 + liveVoice * 0.07 + beat * 0.03);
    }
    if (gyroMid.current) {
      gyroMid.current.rotation.set(
        -0.78 + Math.cos(flow * 0.21) * 0.15,
        -flow * 0.24 * ringSpeed,
        1.1,
      );
      gyroMid.current.scale.setScalar(1.0 + liveVoice * 0.11 + think * 0.04);
    }
    if (gyroInner.current) {
      gyroInner.current.rotation.set(
        0.48 + flow * 0.18 * ringSpeed,
        flow * 0.31 * ringSpeed,
        -0.5,
      );
      gyroInner.current.scale.setScalar(1.0 + beat * 0.06 + liveVoice * 0.08);
    }

    ringColor.set(THEME_RING_HEX[theme]);
    if (listen > 0.01) ringColor.lerp(listenColor, listen * 0.7);
    if (alert > 0.01) ringColor.lerp(alertColor, alert * 0.9);
    const opBoost = 0.14 + charge * 0.22 + liveVoice * 0.28;
    if (gyroMat1.current) {
      gyroMat1.current.color.copy(ringColor);
      gyroMat1.current.opacity = Math.min(0.65, opBoost * 0.72);
    }
    if (gyroMat2.current) {
      gyroMat2.current.color.copy(ringColor);
      gyroMat2.current.opacity = Math.min(0.75, opBoost * 0.95);
    }
    if (gyroMat3.current) {
      gyroMat3.current.color.copy(ringColor);
      gyroMat3.current.opacity = Math.min(0.85, opBoost * 1.15);
    }
  });

  return (
    <>
      <color attach="background" args={['#010308']} />
      <mesh renderOrder={-2} position-z={-0.05}>
        <planeGeometry args={[32, 32]} />
        <shaderMaterial
          vertexShader={vertex}
          fragmentShader={atmosphere}
          args={materialArgs}
          depthWrite={false}
        />
      </mesh>

      <group ref={core}>
        <mesh ref={heart}>
          <sphereGeometry args={[1, 96, 64]} />
          <shaderMaterial
            args={materialArgs}
            vertexShader={shellVertex}
            fragmentShader={surface}
            defines={{ HEART: 1 }}
          />
        </mesh>
        <mesh renderOrder={1}>
          <sphereGeometry
            args={[1, quality === 'Low' ? 112 : 192, quality === 'Low' ? 72 : 120]}
          />
          <shaderMaterial
            vertexShader={shellVertex}
            fragmentShader={surface}
            args={materialArgs}
            transparent
            depthWrite={false}
          />
        </mesh>
        <mesh frustumCulled={false} renderOrder={2}>
          <bufferGeometry>
            <bufferAttribute attach="attributes-position" args={[ribbons.positions, 3]} />
            <bufferAttribute attach="index" args={[ribbons.indices, 1]} />
          </bufferGeometry>
          <shaderMaterial
            args={materialArgs}
            vertexShader={filamentVertex}
            fragmentShader={filamentFragment}
            transparent
            depthWrite={false}
            blending={AdditiveBlending}
            side={DoubleSide}
            forceSinglePass
          />
        </mesh>
      </group>

      {/* Holographic 3D gyroscopic rings */}
      <mesh ref={gyroOuter} renderOrder={3}>
        <torusGeometry args={[1.62, 0.0055, 8, 128]} />
        <meshBasicMaterial
          ref={gyroMat1}
          color={THEME_RING_HEX[theme]}
          transparent
          opacity={0.22}
          blending={AdditiveBlending}
          depthWrite={false}
        />
      </mesh>
      <mesh ref={gyroMid} renderOrder={3}>
        <torusGeometry args={[1.42, 0.004, 8, 112, Math.PI * 1.45]} />
        <meshBasicMaterial
          ref={gyroMat2}
          color={THEME_RING_HEX[theme]}
          transparent
          opacity={0.32}
          blending={AdditiveBlending}
          depthWrite={false}
        />
      </mesh>
      <mesh ref={gyroInner} renderOrder={3}>
        <torusGeometry args={[1.26, 0.0035, 8, 96, Math.PI * 0.85]} />
        <meshBasicMaterial
          ref={gyroMat3}
          color={THEME_RING_HEX[theme]}
          transparent
          opacity={0.38}
          blending={AdditiveBlending}
          depthWrite={false}
        />
      </mesh>

      <lineSegments frustumCulled={false}>
        <bufferGeometry>
          <bufferAttribute attach="attributes-position" args={[arcs, 3]} />
        </bufferGeometry>
        <shaderMaterial
          vertexShader={fieldVertex}
          fragmentShader={fieldFragment}
          args={materialArgs}
          defines={arcDefines}
          blending={AdditiveBlending}
          transparent
          depthWrite={false}
        />
      </lineSegments>
      <points frustumCulled={false}>
        <bufferGeometry>
          <bufferAttribute attach="attributes-position" args={[grains, 3]} />
        </bufferGeometry>
        <shaderMaterial
          vertexShader={fieldVertex}
          fragmentShader={fieldFragment}
          args={materialArgs}
          blending={AdditiveBlending}
          transparent
          depthWrite={false}
        />
      </points>
    </>
  );
}

function WebGLStage(props: StageProps) {
  const responseStart = useRef<number | null>(null);
  const pointerRef = useRef({ x: 0, y: 0 });

  useLayoutEffect(() => {
    if (props.state === 'responding') responseStart.current = performance.now();
  }, [props.state]);

  useEffect(() => {
    const onMove = (e: PointerEvent) => {
      const nx = (e.clientX / Math.max(1, window.innerWidth)) * 2 - 1;
      const ny = (e.clientY / Math.max(1, window.innerHeight)) * 2 - 1;
      pointerRef.current.x = nx;
      pointerRef.current.y = ny;
    };
    const onLeave = () => {
      pointerRef.current.x = 0;
      pointerRef.current.y = 0;
    };
    window.addEventListener('pointermove', onMove, { passive: true });
    window.addEventListener('pointerleave', onLeave);
    return () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerleave', onLeave);
    };
  }, []);

  return (
    <Canvas
      camera={{ fov: 30, near: 0.1, far: 100, position: [0, 0, 7] }}
      dpr={[1, settings[props.quality].dpr]}
      gl={{ alpha: false, antialias: true }}
    >
      <CoreScene {...props} responseStart={responseStart} pointerRef={pointerRef} />
    </Canvas>
  );
}


function StageFallback() {
  return (
    <div className="stage-fallback" role="status">
      <img src="/hero-orb.jpg" alt="Esfera do Jarvis em modo visual simplificado" />
      <p><strong>Modo visual simplificado</strong><br />
        O 3D está indisponível neste navegador. Você pode continuar explorando a interface.
      </p>
    </div>
  );
}

class StageBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() { return this.state.failed ? <StageFallback /> : this.props.children; }
}

export default function Stage(props: StageProps) {
  const [supported] = useState(() => {
    try {
      const canvas = document.createElement('canvas');
      const gl = canvas.getContext('webgl2');
      if (!gl) return false;
      gl.getExtension('WEBGL_lose_context')?.loseContext();
      return true;
    } catch { return false; }
  });
  return supported ? <StageBoundary><WebGLStage {...props} /></StageBoundary> : <StageFallback />;
}
