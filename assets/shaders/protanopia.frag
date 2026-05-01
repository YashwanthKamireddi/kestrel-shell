#version 300 es
// kestrel colour filter (Settings > Accessibility), Hyprland screen shader.
precision highp float;
in vec2 v_texcoord;
uniform sampler2D tex;
out vec4 fragColor;
// Daltonisation: what this kind of colour blindness loses is simulated (Viénot, Brettel
// and Mollon's LMS model), and the lost difference moved into colours still seen.
void main() {
    vec4 c = texture(tex, v_texcoord);
    vec3 rgb = c.rgb;
    float l = 17.8824 * rgb.r + 43.5161 * rgb.g + 4.11935 * rgb.b;
    float m = 3.45565 * rgb.r + 27.1554 * rgb.g + 3.86714 * rgb.b;
    float s = 0.0299566 * rgb.r + 0.184309 * rgb.g + 1.46709 * rgb.b;
    float l2 = 2.02344 * m - 2.52581 * s, m2 = m, s2 = s;
    vec3 sim = vec3(
        0.0809444479 * l2 - 0.130504409 * m2 + 0.116721066 * s2,
        -0.0102485335 * l2 + 0.0540193266 * m2 - 0.113614708 * s2,
        -0.000365296938 * l2 - 0.00412161469 * m2 + 0.693511405 * s2);
    vec3 err = rgb - sim;
    vec3 fix = vec3(0.0, 0.7 * err.r + err.g, 0.7 * err.r + err.b);
    fragColor = vec4(clamp(rgb + fix, 0.0, 1.0), c.a);
}
