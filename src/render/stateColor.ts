// x = listening, y = thinking, z = responding, w = alert. The unweighted
// remainder is idle. uTheme tints the base idle/responding palette.
export const stateColor = /* glsl */ `
uniform vec4 uState;
uniform vec3 uTheme;
vec3 plasmaColor(vec3 col) {
  float energy = dot(col, vec3(.22,.58,.20));
  vec3 themed = mix(col, energy * uTheme * 1.65 + col * uTheme * .55, .72);
  vec3 listening = vec3(themed.r*.62, themed.g*1.24 + .12*energy, themed.b*.96);
  vec3 thinking = vec3(themed.r+.44*themed.b, themed.g*.82, themed.b*1.22);
  vec3 responding = themed + vec3(.26,.34,.38)*energy;
  vec3 alert = vec3(1.68*energy, .72*energy+.10*col.r, .10*energy);
  return themed + uState.x*(listening-themed) + uState.y*(thinking-themed)
                + uState.z*(responding-themed) + uState.w*(alert-themed);
}
`;
