#version 300 es
precision mediump float;

uniform mat4 view;
uniform mat4 projection;

in vec3 i_Position;
//in float i_Age;
//in float i_Life;

//in vec2 i_Coord;
//in vec2 i_TexCoord;

out float v_Age;
out float v_Life;
//out vec2 v_TexCoord;

void main() {
  //float scale = 0.50;
  //vec2 vert_coord = i_Position + (scale * (1.0 - i_Age / i_Life) + 0.05) * 0.1 * i_Coord;
  //v_Age = i_Age;
  //v_Life = i_Life;
  //v_TexCoord = i_TexCoord;
    gl_PointSize = 1.0;
    gl_Position = projection * view * vec4(i_Position, 1.0);
}
