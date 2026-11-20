import { stateColor } from './stateColor';

// Smooth lattice noise shared by the photosphere and the surrounding gas.
const turbulence = /* glsl */ `
float hash(vec3 p) {
  p = fract(p * .1031);
  p += dot(p, p.yzx + 33.33);
  return fract((p.x + p.y) * p.z);
}
float noise3(vec3 p) {
  vec3 i = floor(p), f = fract(p);
  f = f*f*(3.-2.*f);
  return mix(mix(mix(hash(i), hash(i+vec3(1,0,0)), f.x),
                 mix(hash(i+vec3(0,1,0)), hash(i+vec3(1,1,0)), f.x), f.y),
             mix(mix(hash(i+vec3(0,0,1)), hash(i+vec3(1,0,1)), f.x),
                 mix(hash(i+vec3(0,1,1)), hash(i+1.), f.x), f.y), f.z);
}
float folds(vec3 p) {
  return .56*noise3(p) + .28*noise3(p.yzx*2.13+4.2)
       + .13*noise3(p.zxy*4.57+8.1) + .06*noise3(p*9.2);
}
`;

export const vertex = /* glsl */ `
varying vec3 vLocal;
varying vec3 vNormal;
varying vec3 vEye;
void main() {
  vLocal = position;
  vNormal = normalMatrix * normal;
  vec4 eye = modelViewMatrix * vec4(position,1.);
  vEye = -eye.xyz;
  gl_Position = projectionMatrix * eye;
}
`;

// Long folds break the limb at a distance. Narrow crests catch light as the
// camera approaches. Filament footpoints sample this same moving surface.
const relief = /* glsl */ `
uniform float uFlow, uCharge, uPulse;
${turbulence}
float shellRadius(vec3 n) {
  vec3 drift = vec3(.03,-.022,.017)*uFlow;
  float warp = noise3(n*2.7+drift);
  float swell = folds(3.1*n+drift+warp);
  float crest = 1.-abs(sin(9.*n.y+4.*n.x+12.*warp+4.*swell));
  return .96+(.25+.17*uPulse)*(swell-.48)+(.044+.034*uPulse)*crest*crest+.035*uPulse*sin(4.*n.y+2.*warp);
}
`;

export const shellVertex = /* glsl */ `
${relief}
varying vec3 vLocal, vNormal, vEye;
void main() {
  vec3 n = normalize(position);
  vLocal = n;
  vNormal = normalMatrix*n;
  vec4 eye = modelViewMatrix*vec4(n*shellRadius(n),1.);
  vEye = -eye.xyz;
  gl_Position = projectionMatrix*eye;
}
`;

export const filamentVertex = /* glsl */ `
${relief}
uniform float uImpact,uGlint;
varying float vHeat, vId, vCross;
void main() {
  float s = position.x, id = position.z;
  float latitude = asin(1.-2.*(id+.5)/64.);
  float band=mod(id,3.);
  float lon = 2.39996*id+uFlow*(band-1.)*.093;
  vec3 anchor = vec3(cos(latitude)*cos(lon),sin(latitude),cos(latitude)*sin(lon));
  vec3 east = vec3(-sin(lon),0.,cos(lon));
  vec3 north = cross(anchor,east);
  float bearing = id*1.71;
  vec3 tangent = east*cos(bearing)+north*sin(bearing);
  vec3 lateral = cross(anchor,tangent);
  float arch = sin(3.14159*s);
  float width = .22+.38*fract(id*.618);
  float curl = arch*(.055*sin(s*7.+id+.12*uFlow)+.08*sin(id));
  vec3 n = normalize(anchor+tangent*((s-.5)*width)+lateral*curl);
  float height = arch*(.06+.19*fract(id*.7548))*(1.+.12*sin(id+.28*uFlow)+.18*uImpact);
  vec3 p = n*(shellRadius(n)-.14+.14*band+height);
  vec4 eye = modelViewMatrix*vec4(p,1.);
  vec3 axis = mat3(modelViewMatrix)*(tangent*width+anchor*(.48*cos(3.14159*s)));
  vec2 perp = vec2(-axis.y,axis.x);
  eye.xy += perp/max(.001,length(perp))*position.y*(.010+.006*arch);
  gl_Position = projectionMatrix*eye;
  vHeat = arch*(.6+.4*sin(s*12.-uFlow*.7+id))+uGlint*pow(1.-abs(s-.55),5.);
  vId = id;
  vCross = position.y;
}
`;

export const filamentFragment = /* glsl */ `
${stateColor}
varying float vHeat, vId, vCross;
void main() {
  float strand = exp(-3.*abs(vCross));
  vec3 tint = mix(vec3(.04,.67,.92),vec3(.48,.22,.84),step(.78,fract(vId*.618)));
  gl_FragColor = vec4(plasmaColor(tint+vHeat*.24),strand*(.42+.58*vHeat));
  #include <colorspace_fragment>
}
`;

export const surface = /* glsl */ `
${stateColor}
uniform float uFlow, uCharge, uImpact;
varying vec3 vLocal, vNormal, vEye;
${turbulence}
void main() {
  vec3 p = normalize(vLocal);
  vec3 drift = vec3(.04*uFlow, -.025*uFlow, .016*uFlow);
  float deep = folds(2.7*p+drift);
  float skin = folds(4.4*p + 2.2*deep - drift);
  float ribbon = sin(11.*p.y + 5.*p.x + 16.*deep + 6.*skin);
  float seam = pow(1.-abs(ribbon), 3.);
  float pores = noise3(p*65.+skin*6.);
  float window = smoothstep(.38,.91, ribbon + .7*(skin-.5));
  float heart = exp(-4.4*dot(p.xy-vec2(-.17,.12),p.xy-vec2(-.17,.12)));
  vec3 reliefNormal = normalize(cross(dFdx(vEye),dFdy(vEye)));
  vec3 normal = normalize(mix(normalize(vNormal),reliefNormal,.75));
  float limb = pow(1.-clamp(dot(normal,normalize(vEye)),0.,1.),2.5);
  float light = dot(normal,normalize(vec3(-.55,.62,.48)));
  vec3 col = mix(vec3(.002,.004,.027),vec3(.012,.054,.19),skin);
  col += vec3(.018,.30,.61)*seam*(.4+deep);
  col += window*(vec3(.018,.24,.43)+heart*vec3(.86,1.42,1.55))*(.72+.45*pores);
  col += vec3(.37,.10,.70)*smoothstep(.53,.73,deep)*seam;
  col += limb*vec3(.045,.46,.68)*(.2+skin);
  col *= .54+.80*max(light,0.);
  col += vec3(.08,.27,.37)*pow(max(light,0.),9.)*(.25+seam);
  col *= .72+.62*uCharge;
  col += window*heart*uImpact*vec3(1.6,.82,.30);
  #ifdef HEART
    float veins=pow(1.-abs(sin(8.*p.y+15.*deep)),3.);
    col=vec3(.012,.052,.15)+vec3(.035,.42,.65)*skin+vec3(.85,1.6,1.85)*(veins+.5*heart);
    gl_FragColor=vec4(plasmaColor(col),1.);
  #else
    gl_FragColor=vec4(plasmaColor(col),.18+.62*seam+.3*limb+.18*(1.-window));
  #endif
  #include <tonemapping_fragment>
  #include <colorspace_fragment>
}
`;

export const atmosphere = /* glsl */ `
uniform float uFlow, uPhase, uCharge, uMotion;
${stateColor}
varying vec3 vLocal;
${turbulence}
void main() {
  vec2 p = vLocal.xy;
  float r = length(p);
  vec2 dir = p/max(r,.001);
  float gas = folds(vec3(p*.85+dir*.7,uFlow*.028));
  float stream = noise3(vec3(dir*13.,uFlow*.035));
  float corona = exp(-3.2*max(0.,r-.86))*(.18+.8*gas+.4*stream);
  vec3 col = vec3(.004,.066,.14)*corona*(.8+.5*uCharge);
  col += vec3(.028,.011,.068)*exp(-.8*r)*smoothstep(.34,.71,gas);
  float dust = exp(-2.2*abs(p.y-.24*p.x+.3))*exp(-.48*r);
  col += vec3(.009,.031,.061)*dust*gas*gas;
  float radial = .52*r+.12*gas;
  float outgoing = pow(.5+.5*cos(6.283185*(radial-uPhase)),8.);
  float incoming = pow(.5+.5*cos(6.283185*(radial+uPhase)),5.);
  float wave = uState.z*outgoing+.55*uState.x*incoming;
  col += vec3(.034,.24,.36)*wave*uMotion*gas*exp(-.9*abs(r-1.25));
  col += vec3(.018,.062,.12)*uState.w*corona;
  gl_FragColor = vec4(plasmaColor(col),1.);
  #include <tonemapping_fragment>
  #include <colorspace_fragment>
}
`;
