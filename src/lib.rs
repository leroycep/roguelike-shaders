use glam::vec3;
use js_sys::Array;
use std::cell::RefCell;
use std::default::Default;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    WebGl2RenderingContext, WebGlBuffer, WebGlProgram, WebGlShader, WebGlTexture,
    WebGlUniformLocation,
};

mod particle;

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

    // Setup particle systems
    let particle_system = particle::UpdateSystem::new(&context)?;
    let particle_renderer = particle::Render::new(&context)?;

    // Create a particle emitter and a renderer
    let mut fireball = particle_system.create_emitter(
        &context,
        particle::EmitterOptions {
            ..Default::default()
        },
    )?;
    let fireball_gradient = create_gradient_texture(
        &context,
        &[
            glam::f32::Vec4::new(1.0, 1.0, 1.0, 1.0),
            glam::f32::Vec4::new(1.0, 0.83, 0.0, 0.9),
            glam::f32::Vec4::new(0.75, 0.25, 0.05, 0.8),
            glam::f32::Vec4::new(0.18, 0.0, 0.02, 0.5),
            glam::f32::Vec4::new(0.0, 0.0, 0.0, 0.0),
        ],
    )?;

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

        // Update Particles. Skip if the time delta is too large
        if time_delta < 0.15 {
            particle_system.update(&context, &mut fireball, time_delta);
        }

        // Calculate camera position
        let theta = time * (2.0 * std::f32::consts::PI) / 5.0;
        let radius = 1.5;
        let camera_pos = glam::vec3(theta.sin() * radius, 0.5, theta.cos() * radius);
        let view = glam::f32::Mat4::look_at_rh(
            camera_pos,
            glam::vec3(0.0, 0.0, 0.0),
            glam::vec3(0.0, 1.0, 0.0),
        );

        // Render particles
        particle_renderer.render(&context, projection, view, &fireball, &fireball_gradient);

        prev_time = current_time;
        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(g.borrow().as_ref().unwrap());
    Ok(())
}

fn create_gradient_texture(
    gl: &WebGl2RenderingContext,
    gradient: &[glam::f32::Vec4],
) -> Result<WebGlTexture, JsValue> {
    assert!(gradient.len() <= 256);

    let bytes: Vec<_> = gradient
        .iter()
        .map(|c| c.to_array())
        .flatten()
        .map(|f| (f * 255.0) as u8)
        .collect();

    log(&format!(
        "colors len = {}, bytes len = {}",
        gradient.len(),
        bytes.len()
    ));

    let texture = gl
        .create_texture()
        .ok_or_else(|| "Failed to create texture")?;
    gl.bind_texture(WebGl2RenderingContext::TEXTURE_2D, Some(&texture));

    gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
        WebGl2RenderingContext::TEXTURE_2D,
        0,
        WebGl2RenderingContext::RGBA as i32,
        gradient.len() as i32,
        1,
        0,
        WebGl2RenderingContext::RGBA,
        WebGl2RenderingContext::UNSIGNED_BYTE,
        Some(&bytes),
    )?;
    gl.tex_parameteri(
        WebGl2RenderingContext::TEXTURE_2D,
        WebGl2RenderingContext::TEXTURE_WRAP_S,
        WebGl2RenderingContext::CLAMP_TO_EDGE as i32,
    );
    gl.tex_parameteri(
        WebGl2RenderingContext::TEXTURE_2D,
        WebGl2RenderingContext::TEXTURE_WRAP_T,
        WebGl2RenderingContext::CLAMP_TO_EDGE as i32,
    );
    gl.tex_parameteri(
        WebGl2RenderingContext::TEXTURE_2D,
        WebGl2RenderingContext::TEXTURE_MIN_FILTER,
        WebGl2RenderingContext::LINEAR as i32,
    );
    gl.tex_parameteri(
        WebGl2RenderingContext::TEXTURE_2D,
        WebGl2RenderingContext::TEXTURE_MAG_FILTER,
        WebGl2RenderingContext::LINEAR as i32,
    );

    Ok(texture)
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
