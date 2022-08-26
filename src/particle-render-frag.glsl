#version 300 es
precision mediump float;

//uniform sampler2D u_Sprite;

in float v_Age;
in float v_Life;
//in vec2 v_TexCoord;

out vec4 o_FragColor;

vec3 palette( in float t, in vec3 a, in vec3 b, in vec3 c, in vec3 d )
{  return a + b*cos( 6.28318*(c*t+d) ); }

void main() {
  //float t = v_Age / v_Life;
  //vec3 initial_color = vec3(1.0, 0.8, 0.3);
  //vec4 color = vec4(initial_color, 1.0-(v_Age/v_Life));
  //o_FragColor = color * texture(u_Sprite, v_TexCoord);
  float t =  v_Age/v_Life;
  o_FragColor = vec4(
    palette(t,
            vec3(0.5,0.5,0.5),
            vec3(0.5,0.5,0.5),
            vec3(1.0,0.7,0.4),
            vec3(0.0,0.15,0.20)), 1.0 - t);
}
