use crate::renderer::utils::{compile_shader, link_program};
use crate::renderer::gltf_model::GltfModel;
use crate::renderer::shadow_pass::ShadowPass;
use crate::world::entity::Penguin;

pub struct PlacedObjectRenderer {
    shader:    u32,
    mvp_loc:   i32,
    model_loc: i32,
    tex_loc:   i32,
    penguin:   Option<GltfModel>,
}

const VERT: &str = r#"#version 330 core
layout(location = 0) in vec3 aPos;
layout(location = 1) in vec3 aNormal;
layout(location = 2) in vec2 aUV;
uniform mat4 u_mvp;
uniform mat4 u_model;
out vec3 vNormal;
out vec2 vUV;
void main() {
    gl_Position = u_mvp * vec4(aPos, 1.0);
    vNormal = normalize(mat3(u_model) * aNormal);
    vUV = aUV;
}
"#;

const FRAG: &str = r#"#version 330 core
in vec3 vNormal;
in vec2 vUV;
uniform sampler2D u_tex;
out vec4 FragColor;
void main() {
    vec4 col = texture(u_tex, vUV);
    if (col.a < 0.1) discard;
    // Hemisphere lighting: fixed sun direction matching the world default
    vec3 sun = normalize(vec3(0.5, 1.0, 0.3));
    float diff = max(dot(vNormal, sun), 0.0);
    float light = 0.55 + 0.45 * diff;
    FragColor = vec4(col.rgb * light, col.a);
}
"#;

impl PlacedObjectRenderer {
    pub fn new() -> Self {
        let vert = compile_shader(gl::VERTEX_SHADER, VERT).unwrap();
        let frag = compile_shader(gl::FRAGMENT_SHADER, FRAG).unwrap();
        let shader = link_program(vert, frag).unwrap();
        unsafe {
            gl::DeleteShader(vert);
            gl::DeleteShader(frag);
        }

        let mvp_loc   = unsafe { gl::GetUniformLocation(shader, c"u_mvp".as_ptr()) };
        let model_loc = unsafe { gl::GetUniformLocation(shader, c"u_model".as_ptr()) };
        let tex_loc   = unsafe { gl::GetUniformLocation(shader, c"u_tex".as_ptr()) };

        let penguin = match GltfModel::load("assets/models/penguin/source/model.gltf") {
            Ok(m)  => Some(m),
            Err(e) => { eprintln!("[placed_object_renderer] penguin: {e}"); None }
        };

        PlacedObjectRenderer { shader, mvp_loc, model_loc, tex_loc, penguin }
    }

    pub fn draw_penguins(
        &self,
        penguins:   &[Penguin],
        view:       &glam::Mat4,
        projection: &glam::Mat4,
    ) {
        let Some(model) = &self.penguin else { return; };
        if penguins.is_empty() { return; }

        unsafe {
            gl::Enable(gl::DEPTH_TEST);
            gl::Disable(gl::CULL_FACE);
            gl::UseProgram(self.shader);
            gl::Uniform1i(self.tex_loc, 0);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, model.texture);
            gl::BindVertexArray(model.vao);

            for p in penguins {
                let roll = p.move_speed_norm() * (p.anim_time * 5.0).sin() * 0.12;
                let model_mat =
                    glam::Mat4::from_translation(glam::Vec3::from(p.position))
                    * glam::Mat4::from_rotation_y(p.yaw.to_radians())
                    * glam::Mat4::from_rotation_z(roll);
                let mvp = *projection * *view * model_mat;

                gl::UniformMatrix4fv(self.mvp_loc,   1, gl::FALSE, mvp.to_cols_array().as_ptr());
                gl::UniformMatrix4fv(self.model_loc, 1, gl::FALSE, model_mat.to_cols_array().as_ptr());
                gl::DrawElements(gl::TRIANGLES, model.index_count, gl::UNSIGNED_INT,
                    std::ptr::null());
            }

            gl::BindVertexArray(0);
            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::Enable(gl::CULL_FACE);
        }
    }

    pub fn draw_penguin_shadows(&self, penguins: &[Penguin], shadow_pass: &ShadowPass) {
        let Some(model) = &self.penguin else { return; };
        for p in penguins {
            let model_mat =
                glam::Mat4::from_translation(glam::Vec3::from(p.position))
                * glam::Mat4::from_rotation_y(p.yaw.to_radians());
            shadow_pass.draw_solid_mesh_indexed(model.vao, model.index_count, &model_mat);
        }
    }
}

impl Drop for PlacedObjectRenderer {
    fn drop(&mut self) {
        unsafe { gl::DeleteProgram(self.shader); }
    }
}
