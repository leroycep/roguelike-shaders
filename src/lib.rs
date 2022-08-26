use js_sys::Array;
use std::cell::RefCell;
use std::mem::size_of;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    WebGl2RenderingContext, WebGlBuffer, WebGlProgram, WebGlShader, WebGlTexture,
    WebGlUniformLocation, WebGlVertexArrayObject,
};

#[wasm_bindgen(start)]
pub fn display_model() -> Result<(), JsValue> {
    let window = web_sys::window().unwrap();
    let performance = window.performance().unwrap();
    let document = window.document().unwrap();
    let canvas = document.get_element_by_id("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into::<web_sys::HtmlCanvasElement>()?;

    let context = canvas
        .get_context("webgl2")?
        .unwrap()
        .dyn_into::<WebGl2RenderingContext>()?;

    let vert_shader = compile_shader(
        &context,
        WebGl2RenderingContext::VERTEX_SHADER,
        include_str!("particle-render-vert.glsl"),
    )?;
    let frag_shader = compile_shader(
        &context,
        WebGl2RenderingContext::FRAGMENT_SHADER,
        include_str!("particle-render-frag.glsl"),
    )?;
    let render_program = link_program(&context, &vert_shader, &frag_shader, None)?;

    context.use_program(Some(&render_program));
    let projection_uniform = get_uniform(&context, &render_program, "projection")?;
    let view_uniform = get_uniform(&context, &render_program, "view")?;

    // Setup particle buffers
    let particle_buffers = [create_buffer(&context)?, create_buffer(&context)?];
    let num_particles = 800;
    let particle_init_data = generate_initial_particle_data(num_particles, 0.3, 0.9);
    for buffer in &particle_buffers {
        context.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&buffer));
        unsafe {
            let vert_array = js_sys::Float32Array::view(&particle_init_data);

            context.buffer_data_with_array_buffer_view(
                WebGl2RenderingContext::ARRAY_BUFFER,
                &vert_array,
                WebGl2RenderingContext::STATIC_DRAW,
            );
        }
    }

    let mut update = setup_particle_update(&context, &particle_buffers, num_particles)?;

    let projection =
        glam::f32::Mat4::perspective_infinite_rh(f32::to_radians(45.0), 640.0 / 480.0, 0.01);

    let f = Rc::new(RefCell::new(None));
    let g = f.clone();

    let start_time = (performance.now() / 1000.0) as f32;
    let mut prev_time = start_time;
    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        let current_time = (performance.now() / 1000.0) as f32;
        let time_delta = current_time - prev_time;
        let time = current_time - start_time;

        context.clear_color(0.0, 0.0, 0.0, 1.0);
        context.clear(
            WebGl2RenderingContext::COLOR_BUFFER_BIT | WebGl2RenderingContext::DEPTH_BUFFER_BIT,
        );

        run_update(&context, &mut update, &particle_buffers, time_delta);

        let theta = time * (2.0 * std::f32::consts::PI) / 5.0;
        let radius = 1.5;
        let camera_pos = glam::vec3(theta.sin() * radius, 0.5, theta.cos() * radius);

        let view_matrix = glam::f32::Mat4::look_at_rh(
            camera_pos,
            glam::vec3(0.0, 0.0, 0.0),
            glam::vec3(0.0, 1.0, 0.0),
        );

        context.use_program(Some(&render_program));
        context.uniform_matrix4fv_with_f32_array(
            Some(&projection_uniform),
            false,
            &projection.to_cols_array(),
        );
        context.uniform_matrix4fv_with_f32_array(
            Some(&view_uniform),
            false,
            &view_matrix.to_cols_array(),
        );

        context.bind_buffer(
            WebGl2RenderingContext::ARRAY_BUFFER,
            Some(&particle_buffers[update.generation % 2]),
        );

        let num_components = 3 + 1 + 1 + 3;
        let stride = (num_components * size_of::<f32>()) as i32;

        let i_pos = context.get_attrib_location(&render_program, "i_Position") as u32;
        context.enable_vertex_attrib_array(i_pos);
        context.vertex_attrib_pointer_with_i32(
            i_pos,
            3,
            WebGl2RenderingContext::FLOAT,
            false,
            stride,
            0,
        );

        let i_age = context.get_attrib_location(&render_program, "i_Age") as u32;
        context.enable_vertex_attrib_array(i_age);
        context.vertex_attrib_pointer_with_i32(
            i_age,
            1,
            WebGl2RenderingContext::FLOAT,
            false,
            stride,
            (3 * size_of::<f32>()) as i32,
        );

        let i_life = context.get_attrib_location(&render_program, "i_Life") as u32;
        context.enable_vertex_attrib_array(i_life);
        context.vertex_attrib_pointer_with_i32(
            i_life,
            1,
            WebGl2RenderingContext::FLOAT,
            false,
            stride,
            (4 * size_of::<f32>()) as i32,
        );

        context.draw_arrays(WebGl2RenderingContext::POINTS, 0, num_particles);

        context.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, None);

        prev_time = current_time;
        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(g.borrow().as_ref().unwrap());
    Ok(())
}

struct ParticleUpdate {
    program: WebGlProgram,
    vaos: [WebGlVertexArrayObject; 2],
    rg_noise_texture: WebGlTexture,

    num_particles: i32,
    generation: usize,

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

fn setup_particle_update(
    gl: &WebGl2RenderingContext,
    buffers: &[WebGlBuffer; 2],
    num_particles: i32,
) -> Result<ParticleUpdate, JsValue> {
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

    let i_pos = gl.get_attrib_location(&program, "i_Position") as u32;
    let i_age = gl.get_attrib_location(&program, "i_Age") as u32;
    let i_life = gl.get_attrib_location(&program, "i_Life") as u32;
    let i_velocity = gl.get_attrib_location(&program, "i_Velocity") as u32;

    let vaos = [
        gl.create_vertex_array()
            .ok_or("Could not create vertex array")?,
        gl.create_vertex_array()
            .ok_or("Could not create vertex array")?,
    ];

    gl.use_program(Some(&program));
    for (buffer, vao) in buffers.iter().zip(&vaos) {
        gl.bind_vertex_array(Some(vao));

        gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(buffer));

        let num_components = 3 + 1 + 1 + 3;
        let stride = (num_components * size_of::<f32>()) as i32;

        gl.enable_vertex_attrib_array(i_pos);
        gl.vertex_attrib_pointer_with_i32(
            i_pos,
            3,
            WebGl2RenderingContext::FLOAT,
            false,
            stride,
            0,
        );

        gl.enable_vertex_attrib_array(i_age);
        gl.vertex_attrib_pointer_with_i32(
            i_age,
            1,
            WebGl2RenderingContext::FLOAT,
            false,
            stride,
            (3 * size_of::<f32>()) as i32,
        );

        gl.enable_vertex_attrib_array(i_life);
        gl.vertex_attrib_pointer_with_i32(
            i_life,
            1,
            WebGl2RenderingContext::FLOAT,
            false,
            stride,
            (4 * size_of::<f32>()) as i32,
        );

        gl.enable_vertex_attrib_array(i_velocity);
        gl.vertex_attrib_pointer_with_i32(
            i_velocity,
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

    let rg_noise_texture = gl
        .create_texture()
        .ok_or("Could not create texture handle")?;
    gl.bind_texture(WebGl2RenderingContext::TEXTURE_2D, Some(&rg_noise_texture));
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

    Ok(ParticleUpdate {
        vaos,
        num_particles,
        generation: 0,
        rg_noise_texture,

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

fn run_update(
    gl: &WebGl2RenderingContext,
    state: &mut ParticleUpdate,
    buffers: &[WebGlBuffer; 2],
    delta: f32,
) {
    let read = state.generation % 2;
    let write = (state.generation + 1) % 2;

    gl.use_program(Some(&state.program));

    gl.uniform1f(Some(&state.u_timedelta), delta);
    gl.uniform3fv_with_f32_array(Some(&state.u_gravity), &[0.0, -2.0, 0.0]);
    gl.uniform3fv_with_f32_array(Some(&state.u_origin), &[0.0, 0.0, 0.0]);
    gl.uniform1f(Some(&state.u_mintheta), -std::f32::consts::PI);
    gl.uniform1f(Some(&state.u_maxtheta), std::f32::consts::PI);
    gl.uniform1f(Some(&state.u_minspeed), 0.5);
    gl.uniform1f(Some(&state.u_maxspeed), 1.0);

    gl.active_texture(WebGl2RenderingContext::TEXTURE0);
    gl.bind_texture(
        WebGl2RenderingContext::TEXTURE_2D,
        Some(&state.rg_noise_texture),
    );
    gl.uniform1i(Some(&state.u_rgnoise), 0);

    gl.bind_vertex_array(Some(&state.vaos[read]));

    gl.enable(WebGl2RenderingContext::RASTERIZER_DISCARD);
    gl.bind_buffer_base(
        WebGl2RenderingContext::TRANSFORM_FEEDBACK_BUFFER,
        0,
        Some(&buffers[write]),
    );

    gl.begin_transform_feedback(WebGl2RenderingContext::POINTS);
    gl.draw_arrays(WebGl2RenderingContext::POINTS, 0, state.num_particles);
    gl.end_transform_feedback();

    gl.disable(WebGl2RenderingContext::RASTERIZER_DISCARD);
    gl.bind_buffer_base(WebGl2RenderingContext::TRANSFORM_FEEDBACK_BUFFER, 0, None);
    gl.bind_vertex_array(None);

    state.generation += 1;
}

fn window() -> web_sys::Window {
    web_sys::window().expect("no global `window` exists")
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    window()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame` OK");
}

#[wasm_bindgen]
extern "C" {
    // Use `js_namespace` here to bind `console.log(..)` instead of just
    // `log(..)`
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);

}

pub fn compile_shader(
    context: &WebGl2RenderingContext,
    shader_type: u32,
    source: &str,
) -> Result<WebGlShader, String> {
    let shader = context
        .create_shader(shader_type)
        .ok_or_else(|| String::from("Unable to create shader object"))?;
    context.shader_source(&shader, source);
    context.compile_shader(&shader);

    if context
        .get_shader_parameter(&shader, WebGl2RenderingContext::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(shader)
    } else {
        Err(context
            .get_shader_info_log(&shader)
            .unwrap_or_else(|| String::from("Unknown error creating shader")))
    }
}

pub fn link_program(
    context: &WebGl2RenderingContext,
    vert_shader: &WebGlShader,
    frag_shader: &WebGlShader,
    transform_feedback_varyings: Option<&[&str]>,
) -> Result<WebGlProgram, String> {
    let program = context
        .create_program()
        .ok_or_else(|| String::from("Unable to create shader object"))?;

    context.attach_shader(&program, vert_shader);
    context.attach_shader(&program, frag_shader);

    if let Some(varyings) = transform_feedback_varyings {
        let varyings_js: Array = varyings.iter().map(|s| JsValue::from_str(s)).collect();
        context.transform_feedback_varyings(
            &program,
            &varyings_js,
            WebGl2RenderingContext::INTERLEAVED_ATTRIBS,
        );
    }

    context.link_program(&program);

    if context
        .get_program_parameter(&program, WebGl2RenderingContext::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(program)
    } else {
        Err(context
            .get_program_info_log(&program)
            .unwrap_or_else(|| String::from("Unknown error creating program object")))
    }
}

pub fn get_uniform(
    context: &WebGl2RenderingContext,
    program: &WebGlProgram,
    name: &str,
) -> Result<WebGlUniformLocation, String> {
    context
        .get_uniform_location(&program, name)
        .ok_or(format!("Could not get uniform location for {:?}", name))
}

pub fn create_buffer(context: &WebGl2RenderingContext) -> Result<WebGlBuffer, String> {
    let buffer = context
        .create_buffer()
        .ok_or_else(|| format!("Could not create buffer"))?;
    Ok(buffer)
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
