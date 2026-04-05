use std::collections::HashMap;

pub struct Model {
    vao: u32,
    texture: u32,
    vertex_count: i32,
}

pub struct Renderer {
    models: HashMap<String, Model>,
    shader: u32,
}

impl Renderer {
    //fn draw_model(&self, model_name: &str, position: [f32; 3]) {
    //    let model = &self.models[model_name];
    //    gl::BindVertexArray(model.vao);
    //    gl::DrawArrays(gl::TRIANGLES, 0, model.vertex_count);
    //}
}