use js_sys::Array;
use std::cell::RefCell;
use std::collections::HashSet;
use std::mem::size_of;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::WebGlBuffer;
use web_sys::WebGlUniformLocation;
use web_sys::{WebGl2RenderingContext, WebGlProgram, WebGlShader};

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

    /*let particle_update_shader = compile_shader(
        &context,
        WebGl2RenderingContext::VERTEX_SHADER,
        include_str!("particle-update.glsl"),
    )?;
    let passthru_frag_shader = compile_shader(
        &context,
        WebGl2RenderingContext::FRAGMENT_SHADER,
        include_str!("passthru-frag.glsl"),
    )?;
    let update_program = link_program(
        &context,
        &particle_update_shader,
        &passthru_frag_shader,
        &[""],
    )?;*/

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
    let particle_buffers = create_buffers(&context, 2)?;
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

    let projection =
        glam::f32::Mat4::perspective_infinite_rh(f32::to_radians(45.0), 640.0 / 480.0, 0.01);

    let f = Rc::new(RefCell::new(None));
    let g = f.clone();

    let start_time = (performance.now() / 1000.0) as f32;
    let mut i = 0;
    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        let current_time = (performance.now() / 1000.0) as f32;
        let time = current_time - start_time;

        context.clear_color(0.0, 0.0, 0.0, 1.0);
        context.clear(WebGl2RenderingContext::COLOR_BUFFER_BIT);

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
            Some(&particle_buffers[i % 2]),
        );

        let i_pos = context.get_attrib_location(&render_program, "i_Position") as u32;
        context.vertex_attrib_pointer_with_i32(
            i_pos,
            3,
            WebGl2RenderingContext::FLOAT,
            false,
            (5 * size_of::<f32>()) as i32,
            0,
        );
        context.enable_vertex_attrib_array(i_pos);

        context.draw_arrays(WebGl2RenderingContext::POINTS, 0, num_particles);

        i += 1;
        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(g.borrow().as_ref().unwrap());
    Ok(())
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
    context.link_program(&program);

    if let Some(varyings) = transform_feedback_varyings {
        let varyings_js: Array = varyings.iter().map(|s| JsValue::from_str(s)).collect();
        context.transform_feedback_varyings(
            &program,
            &varyings_js,
            WebGl2RenderingContext::INTERLEAVED_ATTRIBS,
        );
    }

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

pub fn create_buffers(
    context: &WebGl2RenderingContext,
    amount: usize,
) -> Result<Vec<WebGlBuffer>, String> {
    let mut buffers = Vec::new();
    for _ in 0..amount {
        buffers.push(
            context
                .create_buffer()
                .ok_or_else(|| format!("Could not create buffer"))?,
        );
    }
    Ok(buffers)
}

fn generate_initial_particle_data(num_parts: i32, min_age: f32, max_age: f32) -> Vec<f32> {
    let mut data = Vec::new();
    for _ in 0..num_parts {
        // position
        data.push(js_sys::Math::random() as f32 - 0.5);
        data.push(js_sys::Math::random() as f32 - 0.5);
        data.push(js_sys::Math::random() as f32 - 0.5);

        let life = min_age + js_sys::Math::random() as f32 * (max_age - min_age);
        // set age to max. life + 1 to ensure the particle gets initialized
        // on first invocation of particle update shader
        data.push(life + 1.0);
        data.push(life);
    }
    data
}
