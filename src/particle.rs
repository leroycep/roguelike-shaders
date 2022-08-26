use crate::{compile_shader, create_buffer, get_uniform, link_program};
use std::mem::size_of;
use wasm_bindgen::JsValue;
use web_sys::{
    WebGl2RenderingContext, WebGlBuffer, WebGlProgram, WebGlTexture, WebGlUniformLocation,
    WebGlVertexArrayObject,
};

// Contains data needed to update a set of particles; it is a "function" that modifies a
// `Emitter` instance.
pub struct UpdateSystem {
    program: WebGlProgram,
    rg_noise: WebGlTexture,

    // vertex attribute locations
    i_pos: u32,
    i_age: u32,
    i_life: u32,
    i_velocity: u32,

    // uniform locations
    u_timedelta: WebGlUniformLocation,
    u_rgnoise: WebGlUniformLocation,
    u_gravity: WebGlUniformLocation,
    u_origin: WebGlUniformLocation,
    u_mintheta: WebGlUniformLocation,
    u_maxtheta: WebGlUniformLocation,
    u_minspeed: WebGlUniformLocation,
    u_maxspeed: WebGlUniformLocation,
}

#[derive(Debug)]
pub struct Emitter {
    num_particles: i32,

    generation: usize,
    buffers: [WebGlBuffer; 2],
    vaos: [WebGlVertexArrayObject; 2],
}

pub struct Render {
    program: WebGlProgram,

    // vertex attribute locations
    i_pos: u32,
    i_age: u32,
    i_life: u32,

    // uniform locations
    u_projection: WebGlUniformLocation,
    u_view: WebGlUniformLocation,
}

impl UpdateSystem {
    pub fn new(gl: &WebGl2RenderingContext) -> Result<UpdateSystem, JsValue> {
        let particle_update_shader = compile_shader(
            gl,
            WebGl2RenderingContext::VERTEX_SHADER,
            include_str!("particle-update.glsl"),
        )?;
        let passthru_frag_shader = compile_shader(
            gl,
            WebGl2RenderingContext::FRAGMENT_SHADER,
            include_str!("passthru-frag.glsl"),
        )?;
        let program = link_program(
            gl,
            &particle_update_shader,
            &passthru_frag_shader,
            Some(&["v_Position", "v_Age", "v_Life", "v_Velocity"]),
        )?;

        let rg_noise = gl
            .create_texture()
            .ok_or("Could not create texture handle")?;
        gl.bind_texture(WebGl2RenderingContext::TEXTURE_2D, Some(&rg_noise));
        gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
            WebGl2RenderingContext::TEXTURE_2D,
            0,
            WebGl2RenderingContext::RG8 as i32,
            512,
            512,
            0,
            WebGl2RenderingContext::RG,
            WebGl2RenderingContext::UNSIGNED_BYTE,
            Some(&generate_random_rg_data(512, 512)),
        )?;
        gl.tex_parameteri(
            WebGl2RenderingContext::TEXTURE_2D,
            WebGl2RenderingContext::TEXTURE_WRAP_S,
            WebGl2RenderingContext::MIRRORED_REPEAT as i32,
        );
        gl.tex_parameteri(
            WebGl2RenderingContext::TEXTURE_2D,
            WebGl2RenderingContext::TEXTURE_WRAP_T,
            WebGl2RenderingContext::MIRRORED_REPEAT as i32,
        );
        gl.tex_parameteri(
            WebGl2RenderingContext::TEXTURE_2D,
            WebGl2RenderingContext::TEXTURE_MIN_FILTER,
            WebGl2RenderingContext::NEAREST as i32,
        );
        gl.tex_parameteri(
            WebGl2RenderingContext::TEXTURE_2D,
            WebGl2RenderingContext::TEXTURE_MAG_FILTER,
            WebGl2RenderingContext::NEAREST as i32,
        );

        Ok(UpdateSystem {
            rg_noise,

            i_pos: gl.get_attrib_location(&program, "i_Position") as u32,
            i_age: gl.get_attrib_location(&program, "i_Age") as u32,
            i_life: gl.get_attrib_location(&program, "i_Life") as u32,
            i_velocity: gl.get_attrib_location(&program, "i_Velocity") as u32,

            u_timedelta: get_uniform(gl, &program, "u_TimeDelta")?,
            u_rgnoise: get_uniform(gl, &program, "u_RgNoise")?,
            u_gravity: get_uniform(gl, &program, "u_Gravity")?,
            u_origin: get_uniform(gl, &program, "u_Origin")?,
            u_mintheta: get_uniform(gl, &program, "u_MinTheta")?,
            u_maxtheta: get_uniform(gl, &program, "u_MaxTheta")?,
            u_minspeed: get_uniform(gl, &program, "u_MinSpeed")?,
            u_maxspeed: get_uniform(gl, &program, "u_MaxSpeed")?,

            program,
        })
    }

    pub fn create_emitter(
        self: &Self,
        gl: &WebGl2RenderingContext,
        num_particles: i32,
        min_age: f32,
        max_age: f32,
    ) -> Result<Emitter, JsValue> {
        let buffers = [create_buffer(gl)?, create_buffer(gl)?];

        let particle_init_data = generate_initial_particle_data(num_particles, min_age, max_age);
        for buffer in &buffers {
            gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&buffer));
            unsafe {
                let vert_array = js_sys::Float32Array::view(&particle_init_data);

                gl.buffer_data_with_array_buffer_view(
                    WebGl2RenderingContext::ARRAY_BUFFER,
                    &vert_array,
                    WebGl2RenderingContext::STATIC_DRAW,
                );
            }
        }

        let vaos = [
            gl.create_vertex_array()
                .ok_or("Could not create vertex array")?,
            gl.create_vertex_array()
                .ok_or("Could not create vertex array")?,
        ];

        gl.use_program(Some(&self.program));
        for (buffer, vao) in buffers.iter().zip(&vaos) {
            gl.bind_vertex_array(Some(vao));

            gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(buffer));

            let num_components = 3 + 1 + 1 + 3;
            let stride = (num_components * size_of::<f32>()) as i32;

            gl.enable_vertex_attrib_array(self.i_pos);
            gl.vertex_attrib_pointer_with_i32(
                self.i_pos,
                3,
                WebGl2RenderingContext::FLOAT,
                false,
                stride,
                0,
            );

            gl.enable_vertex_attrib_array(self.i_age);
            gl.vertex_attrib_pointer_with_i32(
                self.i_age,
                1,
                WebGl2RenderingContext::FLOAT,
                false,
                stride,
                (3 * size_of::<f32>()) as i32,
            );

            gl.enable_vertex_attrib_array(self.i_life);
            gl.vertex_attrib_pointer_with_i32(
                self.i_life,
                1,
                WebGl2RenderingContext::FLOAT,
                false,
                stride,
                (4 * size_of::<f32>()) as i32,
            );

            gl.enable_vertex_attrib_array(self.i_velocity);
            gl.vertex_attrib_pointer_with_i32(
                self.i_velocity,
                3,
                WebGl2RenderingContext::FLOAT,
                false,
                stride,
                (5 * size_of::<f32>()) as i32,
            );
        }
        // reset state
        gl.bind_vertex_array(None);
        gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, None);

        Ok(Emitter {
            generation: 0,
            num_particles,
            buffers,
            vaos,
        })
    }

    pub fn update(self: &Self, gl: &WebGl2RenderingContext, emitter: &mut Emitter, delta: f32) {
        let read = emitter.generation % 2;
        let write = (emitter.generation + 1) % 2;

        gl.use_program(Some(&self.program));

        gl.uniform1f(Some(&self.u_timedelta), delta);
        gl.uniform3fv_with_f32_array(Some(&self.u_gravity), &[0.0, -2.0, 0.0]);
        gl.uniform3fv_with_f32_array(Some(&self.u_origin), &[0.0, 0.0, 0.0]);
        gl.uniform1f(Some(&self.u_mintheta), -std::f32::consts::PI);
        gl.uniform1f(Some(&self.u_maxtheta), std::f32::consts::PI);
        gl.uniform1f(Some(&self.u_minspeed), 0.5);
        gl.uniform1f(Some(&self.u_maxspeed), 1.0);

        gl.active_texture(WebGl2RenderingContext::TEXTURE0);
        gl.bind_texture(WebGl2RenderingContext::TEXTURE_2D, Some(&self.rg_noise));
        gl.uniform1i(Some(&self.u_rgnoise), 0);

        gl.bind_vertex_array(Some(&emitter.vaos[read]));

        gl.enable(WebGl2RenderingContext::RASTERIZER_DISCARD);
        gl.bind_buffer_base(
            WebGl2RenderingContext::TRANSFORM_FEEDBACK_BUFFER,
            0,
            Some(&emitter.buffers[write]),
        );

        gl.begin_transform_feedback(WebGl2RenderingContext::POINTS);
        gl.draw_arrays(WebGl2RenderingContext::POINTS, 0, emitter.num_particles);
        gl.end_transform_feedback();

        gl.disable(WebGl2RenderingContext::RASTERIZER_DISCARD);
        gl.bind_buffer_base(WebGl2RenderingContext::TRANSFORM_FEEDBACK_BUFFER, 0, None);
        gl.bind_vertex_array(None);

        emitter.generation += 1;
    }
}

impl Render {
    pub fn new(gl: &WebGl2RenderingContext) -> Result<Self, JsValue> {
        let vert_shader = compile_shader(
            gl,
            WebGl2RenderingContext::VERTEX_SHADER,
            include_str!("particle-render-vert.glsl"),
        )?;
        let frag_shader = compile_shader(
            gl,
            WebGl2RenderingContext::FRAGMENT_SHADER,
            include_str!("particle-render-frag.glsl"),
        )?;
        let program = link_program(gl, &vert_shader, &frag_shader, None)?;

        Ok(Self {
            i_pos: gl.get_attrib_location(&program, "i_Position") as u32,
            i_age: gl.get_attrib_location(&program, "i_Age") as u32,
            i_life: gl.get_attrib_location(&program, "i_Life") as u32,

            u_projection: get_uniform(gl, &program, "u_Projection")?,
            u_view: get_uniform(gl, &program, "u_View")?,

            program,
        })
    }

    pub fn render(
        self: &Self,
        gl: &WebGl2RenderingContext,
        projection: glam::Mat4,
        view: glam::Mat4,
        emitter: &Emitter,
    ) {
        gl.use_program(Some(&self.program));

        // Bind uniforms
        gl.uniform_matrix4fv_with_f32_array(
            Some(&self.u_projection),
            false,
            &projection.to_cols_array(),
        );
        gl.uniform_matrix4fv_with_f32_array(Some(&self.u_view), false, &view.to_cols_array());

        // Bind particle buffer
        gl.bind_buffer(
            WebGl2RenderingContext::ARRAY_BUFFER,
            Some(&emitter.buffers[(emitter.generation + 1) % 2]),
        );
        let num_components = 3 + 1 + 1 + 3;
        let stride = (num_components * size_of::<f32>()) as i32;

        gl.enable_vertex_attrib_array(self.i_pos);
        gl.vertex_attrib_pointer_with_i32(
            self.i_pos,
            3,
            WebGl2RenderingContext::FLOAT,
            false,
            stride,
            0,
        );

        gl.enable_vertex_attrib_array(self.i_age);
        gl.vertex_attrib_pointer_with_i32(
            self.i_age,
            1,
            WebGl2RenderingContext::FLOAT,
            false,
            stride,
            (3 * size_of::<f32>()) as i32,
        );

        gl.enable_vertex_attrib_array(self.i_life);
        gl.vertex_attrib_pointer_with_i32(
            self.i_life,
            1,
            WebGl2RenderingContext::FLOAT,
            false,
            stride,
            (4 * size_of::<f32>()) as i32,
        );

        // Draw particles
        gl.draw_arrays(WebGl2RenderingContext::POINTS, 0, emitter.num_particles);

        // Reset bindings
        gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, None);
    }
}

fn generate_random_rg_data(width: usize, height: usize) -> Vec<u8> {
    let mut data = Vec::new();
    for _ in 0..(width * height) {
        // position
        data.push((js_sys::Math::random() * 255.0) as u8);
        data.push((js_sys::Math::random() * 255.0) as u8);
    }
    data
}

fn generate_initial_particle_data(num_parts: i32, min_age: f32, max_age: f32) -> Vec<f32> {
    let mut data = Vec::new();
    for _ in 0..num_parts {
        // position
        data.push(0.0);
        data.push(0.0);
        data.push(0.0);

        let life = min_age + js_sys::Math::random() as f32 * (max_age - min_age);
        // set age to max. life + 1 to ensure the particle gets initialized
        // on first invocation of particle update shader
        data.push(life + 1.0); // age
        data.push(life); // life

        // velocity
        data.push(0.0);
        data.push(0.0);
        data.push(0.0);
    }
    data
}
