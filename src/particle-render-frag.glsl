#version 300 es
precision mediump float;

uniform sampler2D u_Gradient;

//uniform sampler2D u_Sprite;

in float v_Age;
in float v_Life;
//in vec2 v_TexCoord;

out vec4 o_FragColor;

void main() {
  //float t = v_Age / v_Life;
  //vec3 initial_color = vec3(1.0, 0.8, 0.3);
  //vec4 color = vec4(initial_color, 1.0-(v_Age/v_Life));
  //o_FragColor = color * texture(u_Sprite, v_TexCoord);
  vec2 texture_coord = vec2(v_Age/v_Life, 0.5);
  o_FragColor = texture(u_Gradient, texture_coord);
}
