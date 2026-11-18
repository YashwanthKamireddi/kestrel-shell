#version 300 es
// kestrel colour filter (Settings > Accessibility), Hyprland screen shader.
precision highp float;
in vec2 v_texcoord;
uniform sampler2D tex;
out vec4 fragColor;
void main() {
    vec4 c = texture(tex, v_texcoord);
    fragColor = vec4(vec3(1.0) - c.rgb, c.a);
}
