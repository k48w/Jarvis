import { stateColor } from './stateColor';

// Each vertex stores angle, stream index and grain offset in its position.
// Keeping the stream parameters common to dust and arcs registers the two.
export function fieldVertices(count: number, arcs: boolean) {
  let seed = 0x2a4139;
  function random() {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    return seed / 4294967296;
  }
  const data = new Float32Array(count * 3);
  const arcLength = 128;
  for (let i = 0; i < count; i++) {
    const stream = arcs ? Math.floor(i / arcLength) : Math.floor(random() * 48);
    const step = i % arcLength;
    const angle = arcs
      ? stream * 2.39 + (Math.floor(step / 2) + step % 2) * .029
      : random() * Math.PI * 2;
    data.set([angle, stream, arcs ? .5 : random()], i * 3);
  }
  return data;
}

export function filamentRibbons() {
  const steps = 40;
  const positions = [];
  const indices = [];
  for (let id = 0; id < 64; id++) {
    for (let i = 0; i <= steps; i++) {
      const index = positions.length / 3;
      positions.push(i / steps, -1, id, i / steps, 1, id);
      if (i < steps) indices.push(index, index + 1, index + 2, index + 2, index + 1, index + 3);
    }
  }
  return { positions: new Float32Array(positions), indices: new Uint16Array(indices) };
}

export const fieldVertex = /* glsl */ `
uniform float uFlow, uCharge, uPhase, uMotion, uPixel, uGlint;
uniform vec4 uState;
varying float vLight, vHue;
float rand(float n) { return fract(sin(n*43.21+1.7)*437.545); }
void main() {
  float id = position.y;
  float grain = position.z-.5;
  float radius = 1.12+3.65*pow(rand(id),1.8);
  float angle = position.x + uFlow*(.08+.19/radius) + .14*uCharge;
  float spread = .075+pow(rand(id+4.),4.)*.72;
  radius += grain*spread + .17*sin(3.*angle+id);
  radius -= .18*uCharge;
  float outbound = .5+.5*cos(6.283185*(radius*.55-uPhase));
  float inbound = .5+.5*cos(6.283185*(radius*.55+uPhase));
  float lift = uMotion*(outbound*uState.z + inbound*uState.x);
  radius += .18*lift-.14*uState.x;
  angle += .23*uState.y*sin(radius*1.7+uFlow*.23);
  vec3 p = vec3(cos(angle)*radius, sin(angle)*radius, grain*spread*.7);
  // Rotate orbital planes about X, then tip their common axis diagonally.
  float tilt = .97+.65*rand(id+11.)+.34*uState.y*sin(id+uFlow*.15);
  p.yz = mat2(cos(tilt),sin(tilt),-sin(tilt),cos(tilt))*p.yz;
  float roll = .14+.42*rand(id+8.);
  p.xy = mat2(cos(roll),sin(roll),-sin(roll),cos(roll))*p.xy;
  #ifndef ARC
    // Sparse front and rear curtains pass the lens at different speeds.
    float front=step(45.,id), rear=1.-step(6.,id);
    float drift=position.x+uFlow*(.075*front+.014*rear);
    if(front+rear>.5){
      p=vec3(6.*cos(drift),3.*sin(position.x*2.+id+.14*drift),front*2.65-rear*3.5+grain*1.8);
    }
  #endif
  vec4 eye = modelViewMatrix*vec4(p,1.);
  gl_Position = projectionMatrix*eye;
  vHue = rand(id+7.);
  vLight = (.26+.74*pow(.5+.5*cos(2.*angle+id),2.))*(.78+.5*uCharge+lift);
  #ifdef ARC
    vLight *= .28*pow(sin(3.14159*clamp((position.x-id*2.39)/1.856,0.,1.)),2.);
  #else
    gl_PointSize = clamp(uPixel*(.8+3.8*pow(position.z,5.)+3.3*front)/max(2.,-eye.z),1.,11.);
    vLight *= (.45+.55*position.z)*(1.-.72*front-.5*rear)*(1.+1.4*uGlint*step(.80,position.z));
  #endif
}
`;

export const fieldFragment = /* glsl */ `
varying float vLight, vHue;
uniform float uImpact;
${stateColor}
void main() {
  float opacity = vLight;
  #ifndef ARC
    vec2 uv = gl_PointCoord-.5;
    opacity *= exp(-13.*dot(uv,uv))*(1.-smoothstep(.18,.25,dot(uv,uv)));
  #endif
  vec3 col = mix(vec3(.12,.62,.83),vec3(.49,.25,.83),smoothstep(.64,.92,vHue));
  col = mix(col,vec3(1.,.55,.2),uImpact*smoothstep(.68,.89,vHue));
  gl_FragColor = vec4(plasmaColor(col),opacity);
  #include <colorspace_fragment>
}
`;
