use crate::renderer::utils::{compile_shader, link_program};
use crate::renderer::gltf_model::GltfModel;
use crate::renderer::shadow_pass::{ShadowPass, NUM_CASCADES, CASCADE_ENDS};
use crate::world::entity::Penguin;

pub struct PlacedObjectRenderer {
    shader:              u32,
    mvp_loc:             i32,
    model_loc:           i32,
    tex_loc:             i32,
    // fog
    fog_start_loc:       i32,
    fog_end_loc:         i32,
    fog_override_loc:    i32,
    fog_color_ovr_loc:   i32,
    screen_size_loc:     i32,
    sky_sampler_loc:     i32,
    // lighting
    ambient_light_loc:   i32,
    directional_light_loc: i32,
    light_dir_loc:       i32,
    block_light_loc:     i32,
    // shadows
    shadow_maps_loc:     i32,
    light_space_loc:     i32,
    cascade_ends_loc:    i32,
    texel_sizes_loc:     i32,

    penguin: Option<GltfModel>,
}

const VERT: &str = r#"#version 330 core
layout(location = 0) in vec3 aPos;
layout(location = 1) in vec3 aNormal;
layout(location = 2) in vec2 aUV;
uniform mat4 u_mvp;
uniform mat4 u_model;
out vec3 vNormal;
out vec2 vUV;
out vec3 vWorldPos;
out float fragDist;
void main() {
    vec4 worldPos = u_model * vec4(aPos, 1.0);
    gl_Position   = u_mvp * vec4(aPos, 1.0);
    vNormal   = normalize(mat3(u_model) * aNormal);
    vUV       = aUV;
    vWorldPos = worldPos.xyz;
    fragDist  = length(gl_Position.xyz);
}
"#;

const FRAG: &str = r#"#version 330 core
#define NUM_CASCADES 3
in vec3 vNormal;
in vec2 vUV;
in vec3 vWorldPos;
in float fragDist;

uniform sampler2D        u_tex;
uniform float            u_fog_start;
uniform float            u_fog_end;
uniform float            u_fog_override;
uniform vec3             u_fog_color_override;
uniform vec2             u_screen_size;
uniform sampler2D        u_sky;
uniform float            u_ambient_light;
uniform float            u_directional_light;
uniform vec3             u_light_dir;
uniform float            u_block_light;
uniform sampler2DArray   u_shadow_maps;
uniform mat4             u_light_space[NUM_CASCADES];
uniform float            u_cascade_ends[NUM_CASCADES];
uniform float            u_texel_sizes[NUM_CASCADES];

out vec4 FragColor;

float calcShadow(vec3 worldPos, vec3 normal, float viewDist) {
    int cascade = NUM_CASCADES - 1;
    for (int i = 0; i < NUM_CASCADES; i++) {
        if (viewDist < u_cascade_ends[i]) { cascade = i; break; }
    }
    vec4 lsPos = u_light_space[cascade] * vec4(worldPos, 1.0);
    vec3 proj   = lsPos.xyz / lsPos.w * 0.5 + 0.5;
    if (proj.z > 1.0) return 1.0;
    float bias  = max(u_texel_sizes[cascade] * 2.0 * (1.0 - max(dot(normal, u_light_dir), 0.0)), u_texel_sizes[cascade]);
    float shadow = 0.0;
    float ts     = 1.0 / 1024.0;
    for (int x = -1; x <= 1; x++) {
        for (int y = -1; y <= 1; y++) {
            float depth = texture(u_shadow_maps, vec3(proj.xy + vec2(x,y)*ts, float(cascade))).r;
            shadow += (proj.z - bias > depth) ? 1.0 : 0.0;
        }
    }
    return shadow / 9.0;
}

void main() {
    vec4 col = texture(u_tex, vUV);
    if (col.a < 0.1) discard;

    float shadow = calcShadow(vWorldPos, vNormal, fragDist);
    float cave_amb = 0.03;
    float sun    = u_ambient_light + u_directional_light * (1.0 - shadow);
    float light  = mix(cave_amb, sun, u_block_light);
    vec3  lit    = col.rgb * light;

    // fog
    float fog_factor = clamp((fragDist - u_fog_start) / (u_fog_end - u_fog_start), 0.0, 1.0);
    vec2  uv_sky   = gl_FragCoord.xy / u_screen_size;
    vec3  skyFog   = texture(u_sky, uv_sky).rgb;
    vec3  fogColor = mix(skyFog, u_fog_color_override, u_fog_override);

    FragColor = vec4(mix(lit, fogColor, fog_factor), col.a);
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

        let fog_start_loc     = unsafe { gl::GetUniformLocation(shader, c"u_fog_start".as_ptr()) };
        let fog_end_loc       = unsafe { gl::GetUniformLocation(shader, c"u_fog_end".as_ptr()) };
        let fog_override_loc  = unsafe { gl::GetUniformLocation(shader, c"u_fog_override".as_ptr()) };
        let fog_color_ovr_loc = unsafe { gl::GetUniformLocation(shader, c"u_fog_color_override".as_ptr()) };
        let screen_size_loc   = unsafe { gl::GetUniformLocation(shader, c"u_screen_size".as_ptr()) };
        let sky_sampler_loc   = unsafe { gl::GetUniformLocation(shader, c"u_sky".as_ptr()) };

        let ambient_light_loc     = unsafe { gl::GetUniformLocation(shader, c"u_ambient_light".as_ptr()) };
        let directional_light_loc = unsafe { gl::GetUniformLocation(shader, c"u_directional_light".as_ptr()) };
        let light_dir_loc         = unsafe { gl::GetUniformLocation(shader, c"u_light_dir".as_ptr()) };
        let block_light_loc       = unsafe { gl::GetUniformLocation(shader, c"u_block_light".as_ptr()) };

        let shadow_maps_loc  = unsafe { gl::GetUniformLocation(shader, c"u_shadow_maps".as_ptr()) };
        let light_space_loc  = unsafe { gl::GetUniformLocation(shader, c"u_light_space".as_ptr()) };
        let cascade_ends_loc = unsafe { gl::GetUniformLocation(shader, c"u_cascade_ends".as_ptr()) };
        let texel_sizes_loc  = unsafe { gl::GetUniformLocation(shader, c"u_texel_sizes".as_ptr()) };

        let penguin = match GltfModel::load("assets/models/penguin/source/model.gltf") {
            Ok(m)  => Some(m),
            Err(e) => { eprintln!("[placed_object_renderer] penguin: {e}"); None }
        };

        PlacedObjectRenderer {
            shader, mvp_loc, model_loc, tex_loc,
            fog_start_loc, fog_end_loc, fog_override_loc, fog_color_ovr_loc,
            screen_size_loc, sky_sampler_loc,
            ambient_light_loc, directional_light_loc, light_dir_loc, block_light_loc,
            shadow_maps_loc, light_space_loc, cascade_ends_loc, texel_sizes_loc,
            penguin,
        }
    }

    pub fn draw_penguins(
        &self,
        penguins:         &[Penguin],
        view:             &glam::Mat4,
        projection:       &glam::Mat4,
        fog_start:        f32,
        fog_end:          f32,
        screen_w:         f32,
        screen_h:         f32,
        sky_tex:          u32,
        fog_override:     f32,
        fog_color_override: glam::Vec3,
        ambient_light:    f32,
        directional_light: f32,
        sun_dir:          glam::Vec3,
        shadow_tex:       u32,
        light_space:      &[glam::Mat4; NUM_CASCADES],
        texel_sizes:      &[f32; NUM_CASCADES],
    ) {
        let Some(model) = &self.penguin else { return; };
        if penguins.is_empty() { return; }

        unsafe {
            gl::Enable(gl::DEPTH_TEST);
            gl::Disable(gl::CULL_FACE);
            gl::UseProgram(self.shader);

            // fog + screen
            gl::Uniform1f(self.fog_start_loc,   fog_start);
            gl::Uniform1f(self.fog_end_loc,     fog_end);
            gl::Uniform1f(self.fog_override_loc, fog_override);
            gl::Uniform3f(self.fog_color_ovr_loc,
                fog_color_override.x, fog_color_override.y, fog_color_override.z);
            gl::Uniform2f(self.screen_size_loc, screen_w, screen_h);

            // lighting
            gl::Uniform1f(self.ambient_light_loc,     ambient_light);
            gl::Uniform1f(self.directional_light_loc, directional_light);
            gl::Uniform3f(self.light_dir_loc, sun_dir.x, sun_dir.y, sun_dir.z);
            // block_light is uploaded per-entity in the draw loop below

            // shadow cascade
            gl::Uniform1fv(self.cascade_ends_loc, NUM_CASCADES as i32, CASCADE_ENDS.as_ptr());
            gl::Uniform1fv(self.texel_sizes_loc,  NUM_CASCADES as i32, texel_sizes.as_ptr());
            gl::UniformMatrix4fv(self.light_space_loc, NUM_CASCADES as i32, gl::FALSE,
                light_space[0].as_ref().as_ptr());

            // texture unit 0: model diffuse
            gl::Uniform1i(self.tex_loc, 0);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, model.texture);

            // texture unit 4: sky (fog colour)
            gl::Uniform1i(self.sky_sampler_loc, 4);
            gl::ActiveTexture(gl::TEXTURE4);
            gl::BindTexture(gl::TEXTURE_2D, sky_tex);

            // texture unit 5: shadow map array
            gl::Uniform1i(self.shadow_maps_loc, 5);
            gl::ActiveTexture(gl::TEXTURE5);
            gl::BindTexture(gl::TEXTURE_2D_ARRAY, shadow_tex);

            gl::BindVertexArray(model.vao);

            for p in penguins.iter().filter(|e| e.def.identifier == "penguin") {
                gl::Uniform1f(self.block_light_loc, p.block_light);
                let roll = p.move_speed_norm() * (p.anim_time * 5.0).sin() * 0.12;
                let model_mat =
                    glam::Mat4::from_translation(glam::Vec3::from(p.position))
                    * glam::Mat4::from_rotation_y(p.yaw.to_radians())
                    * glam::Mat4::from_rotation_z(roll)
                    * glam::Mat4::from_scale(glam::Vec3::splat(p.def.render_scale));
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
        for p in penguins.iter().filter(|e| e.def.identifier == "penguin") {
            let model_mat =
                glam::Mat4::from_translation(glam::Vec3::from(p.position))
                * glam::Mat4::from_rotation_y(p.yaw.to_radians())
                * glam::Mat4::from_scale(glam::Vec3::splat(p.def.render_scale));
            shadow_pass.draw_solid_mesh_indexed(model.vao, model.index_count, &model_mat);
        }
    }
}

impl Drop for PlacedObjectRenderer {
    fn drop(&mut self) {
        unsafe { gl::DeleteProgram(self.shader); }
    }
}
