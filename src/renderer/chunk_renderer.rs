use crate::renderer::shadow_pass::NUM_CASCADES;
use crate::renderer::utils::{compile_shader, create_block_atlas};
use gl::types::GLchar;
use std::ptr;
use crate::world::chunk::Chunk;

struct UniformLocations {
    model: i32,
    view: i32,
    projection: i32,
    use_textures: i32,
    fog_start: i32,
    fog_end: i32,
    transparent_pass: i32,
    /// Base location of `lightSpaceMatrices[0]`; the array is contiguous so we
    /// set all NUM_CASCADES with a single UniformMatrix4fv call.
    light_space_matrices: i32,
    shadow_maps: i32,        // sampler2DArray (one layer per cascade)
    cascade_ends: i32,       // base of float array
    shadow_texel_sizes: i32, // base of float array
    light_dir: i32,
    fog_color: i32,
    fog_override: i32,
    torch_pos: i32,
    torch_strength: i32,
    ambient_light: i32,
    directional_light: i32,
    time: i32,
    camera_pos: i32,
    depth_sampler: i32,
    proj_near: i32,
    proj_far: i32,
    screen_size: i32,
    refraction_sampler: i32,
    water_level: i32,
    sky_sampler: i32,
}


pub struct ChunkRenderer {
    shader: u32,
    texture_atlas: u32,
    uniforms: UniformLocations,
    depth_tex: u32,
    refraction_tex: u32,
    sky_tex: u32,
}

impl ChunkRenderer {
    pub fn new() -> Result<Self, String> {
        unsafe {
            // Vertex shader: pass world position and normal through to the
            // fragment shader, which picks the cascade per fragment.
            let vertex_shader = compile_shader(
                gl::VERTEX_SHADER,
                r#"#version 330 core
                layout(location = 0) in vec3 aPos;
                layout(location = 1) in vec3 aColor;
                layout(location = 2) in vec2 aTexCoord;
                layout(location = 3) in vec3 aNormal;
                layout(location = 4) in float aSkyLight;

                out vec3 ourColor;
                out vec2 TexCoord;
                out float fragDist;
                out vec3 vWorldPos;
                out vec3 vNormal;
                out float vSkyLight;

                uniform mat4  model;
                uniform mat4  view;
                uniform mat4  projection;
                uniform float u_time;
                uniform bool  transparent_pass;

                float gerstnerY(vec2 xz, vec2 dir, float A, float k, float spd) {
                    return A * sin(dot(dir, xz) * k + u_time * spd);
                }

                void main() {
                    vec4 worldPos = model * vec4(aPos, 1.0);

                    // Gerstner wave Y-displacement on the water surface top face.
                    // Only Y is displaced to avoid visible seams at chunk borders.
                    if (transparent_pass && aNormal.y > 0.5) {
                        float dy = 0.0;
                        dy += gerstnerY(worldPos.xz, vec2( 0.9578,  0.2873), 0.035, 1.5, 1.5);
                        dy += gerstnerY(worldPos.xz, vec2( 0.0,     1.0),    0.025, 2.0, 1.0);
                        dy += gerstnerY(worldPos.xz, vec2(-0.848,   0.530),  0.015, 3.0, 2.0);
                        worldPos.y += dy;
                    }

                    vec4 viewPos = view * worldPos;
                    gl_Position  = projection * viewPos;
                    ourColor     = aColor;
                    TexCoord     = aTexCoord;
                    fragDist     = abs(viewPos.z);
                    vWorldPos    = worldPos.xyz;
                    // Chunks are translation-only, so mat3(model) == identity.
                    vNormal      = mat3(model) * aNormal;
                    vSkyLight    = aSkyLight;
                }"#
            )?;

            let fragment_shader = compile_shader(
                gl::FRAGMENT_SHADER,
                r#"#version 330 core
                #define NUM_CASCADES 3

                in vec3 ourColor;
                in vec2 TexCoord;
                in float fragDist;
                in vec3 vWorldPos;
                in vec3 vNormal;
                in float vSkyLight;
                out vec4 FragColor;

                uniform sampler2D      texture_atlas;
                uniform sampler2DArray shadowMaps;
                uniform sampler2D      u_depth_sampler;
                uniform sampler2D      u_refraction_sampler;
                uniform sampler2D      u_sky_sampler;
                uniform mat4  lightSpaceMatrices[NUM_CASCADES];
                uniform mat4  view;
                uniform mat4  projection;
                uniform float cascadeEnds[NUM_CASCADES];
                uniform float shadowTexelSizes[NUM_CASCADES];
                uniform bool  use_textures;
                uniform float fog_start;
                uniform float fog_end;
                uniform vec3  fog_color;
                uniform float u_fog_override;
                uniform bool  transparent_pass;
                uniform vec3  lightDir;
                uniform float ambientLight;
                uniform float directionalLight;
                uniform float u_time;
                uniform vec3  u_camera_pos;
                uniform float u_proj_near;
                uniform float u_proj_far;
                uniform vec2  u_screen_size;
                uniform float u_water_level;
                uniform vec3  u_torch_pos;
                uniform float u_torch_strength;

                vec2 waveGrad(vec2 pos, vec2 dir, float k, float amp, float speed) {
                    return dir * (k * amp * cos(dot(dir, pos) * k + u_time * speed));
                }

                float linearDepth(float depth) {
                    float z = depth * 2.0 - 1.0;
                    return (2.0 * u_proj_near * u_proj_far)
                         / (u_proj_far + u_proj_near - z * (u_proj_far - u_proj_near));
                }

                float calcShadow(vec3 worldPos, vec3 normal, float viewDist) {
                    if (dot(normalize(normal), -normalize(lightDir)) <= 0.0) return 1.0;
                    int cascade = NUM_CASCADES - 1;
                    for (int i = 0; i < NUM_CASCADES - 1; i++) {
                        if (viewDist < cascadeEnds[i]) { cascade = i; break; }
                    }
                    vec3 offsetWorld = worldPos + normalize(normal) * shadowTexelSizes[cascade];
                    vec4 fragPosLS   = lightSpaceMatrices[cascade] * vec4(offsetWorld, 1.0);
                    vec3 projCoords  = fragPosLS.xyz / fragPosLS.w * 0.5 + 0.5;
                    if (projCoords.z > 1.0) return 0.0;
                    float currentDepth = projCoords.z;
                    float shadow = 0.0;
                    const vec2 texelSize = vec2(1.0 / 2048.0);
                    for (int x = -1; x <= 1; ++x)
                        for (int y = -1; y <= 1; ++y) {
                            shadow += currentDepth > texture(shadowMaps,
                                vec3(projCoords.xy + vec2(x, y) * texelSize, cascade)).r ? 1.0 : 0.0;
                        }
                    return shadow / 9.0;
                }

                void main() {
                    // Screen UV and sky colour computed once; used for fog and SSR fallback.
                    vec2 screenUV = gl_FragCoord.xy / u_screen_size;
                    vec3 skyFog   = texture(u_sky_sampler, screenUV).rgb;
                    vec3 fogColor = mix(skyFog, fog_color, u_fog_override);

                    vec4 texSample;
                    if (use_textures) {
                        texSample = texture(texture_atlas, TexCoord);
                        if (texSample.a < 0.1) discard;
                    } else {
                        texSample = vec4(1.0);
                    }
                    if (!transparent_pass && texSample.a < 0.99) discard;
                    if ( transparent_pass && texSample.a >= 0.99) discard;

                    float shadow = calcShadow(vWorldPos, vNormal, fragDist);
                    float outdoor_sun = ambientLight + directionalLight * (1.0 - shadow);
                    // Caves receive a fraction of ambient only; no directional sun.
                    float cave_ambient = ambientLight * 0.25;
                    float sun_light = mix(cave_ambient, outdoor_sun, vSkyLight);
                    float torch_dist = length(vWorldPos - u_torch_pos);
                    float torch_atten = max(0.0, 1.0 - torch_dist / 12.0);
                    torch_atten = torch_atten * sqrt(torch_atten);
                    vec3 torch_contrib = torch_atten * u_torch_strength * 1.8 * vec3(1.0, 0.82, 0.55);
                    vec3  color  = texSample.rgb * ourColor * (vec3(sun_light) + torch_contrib);

                    // ── Water surface ────────────────────────────────────────────
                    if (transparent_pass && vNormal.y > 0.5) {
                        // Multi-layer wave normal
                        vec2 pos2d = vec2(vWorldPos.x, vWorldPos.z);
                        vec2 grad  = vec2(0.0);
                        grad += waveGrad(pos2d, normalize(vec2( 0.7,  0.7)), 1.5, 0.05,  0.8);
                        grad += waveGrad(pos2d, normalize(vec2( 1.0,  0.0)), 0.4, 0.08,  0.3);
                        grad += waveGrad(pos2d, normalize(vec2(-0.3,  1.0)), 4.0, 0.015, 1.5);
                        vec3 wNormal = normalize(vec3(-grad.x, 1.0, -grad.y));

                        // Fresnel (Schlick)
                        vec3  viewDir = normalize(u_camera_pos - vWorldPos);
                        float NdotV   = max(0.0, dot(wNormal, viewDir));
                        float fresnel = 0.02 + 0.98 * pow(1.0 - NdotV, 5.0);

                        // Blinn-Phong specular
                        vec3  L    = normalize(-lightDir);
                        vec3  H    = normalize(L + viewDir);
                        float spec = pow(max(0.0, dot(wNormal, H)), 128.0)
                                   * directionalLight * (1.0 - shadow);

                        // Water column depth
                        float rawDepth   = texture(u_depth_sampler, screenUV).r;
                        float waterDepth = max(0.0, linearDepth(rawDepth) - fragDist);
                        float depthFade  = 1.0 - exp(-waterDepth * 0.6);

                        // Refraction: distorted underwater view driven by wave normal
                        vec2 refractUV = clamp(screenUV + wNormal.xz * 0.05, 0.001, 0.999);
                        if (linearDepth(texture(u_depth_sampler, refractUV).r) < fragDist)
                            refractUV = screenUV;
                        vec3 refractColor = texture(u_refraction_sampler, refractUV).rgb;

                        // Water body: refracted terrain tinted by depth and water colour
                        vec3 waterBody = mix(refractColor, vec3(0.0, 0.1, 0.35) * sun_light, depthFade * 0.6);
                        waterBody      = mix(waterBody, waterBody * ourColor * 1.4, 0.25);

                        // SSR: ray-march the reflected direction through screen space
                        vec3  fragViewPos = (view * vec4(vWorldPos, 1.0)).xyz;
                        vec3  reflViewDir = mat3(view) * reflect(-viewDir, wNormal);
                        vec3  ssrColor    = skyFog;
                        float ssrWeight   = 0.0;
                        for (int i = 1; i <= 16; i++) {
                            vec3 sVP   = fragViewPos + reflViewDir * (float(i) * 0.5);
                            vec4 sClip = projection * vec4(sVP, 1.0);
                            if (sClip.w <= 0.0) break;
                            vec2 sUV = (sClip.xy / sClip.w) * 0.5 + 0.5;
                            if (any(lessThan(sUV, vec2(0.01))) || any(greaterThan(sUV, vec2(0.99)))) break;
                            float rayDepth = -sVP.z;
                            float sDepth   = linearDepth(texture(u_depth_sampler, sUV).r);
                            if (rayDepth > sDepth + 0.1 && rayDepth - sDepth < 3.0) {
                                ssrColor  = texture(u_refraction_sampler, sUV).rgb;
                                ssrWeight = 1.0;
                                break;
                            }
                        }

                        // Fresnel blends refracted body ↔ SSR/sky
                        color  = mix(waterBody, mix(skyFog, ssrColor, ssrWeight), fresnel * 0.65);
                        color += vec3(spec);

                        // Shoreline foam
                        color = mix(color, vec3(1.0), (1.0 - smoothstep(0.0, 0.7, waterDepth)) * 0.75);

                        // Deepen alpha with water column depth
                        texSample.a = min(0.92, texSample.a + depthFade * 0.25);
                    }

                    // ── Caustics on underwater opaque terrain ────────────────────
                    // Only horizontal (top) faces: caustic light travels downward,
                    // not sideways, so vertical faces at the shoreline must be excluded.
                    if (!transparent_pass && vWorldPos.y < u_water_level && vNormal.y > 0.3) {
                        float causticFade = 1.0 - clamp((u_water_level - vWorldPos.y) / 8.0, 0.0, 1.0);
                        vec2  wxz = vWorldPos.xz;
                        float t   = u_time * 1.2;
                        float c1  = sin(length(wxz - vec2(sin(t*0.6)*3.0, cos(t*0.5)*3.0)) * 5.0 - t*2.0);
                        float c2  = sin(length(wxz + vec2(cos(t*0.7)*3.0, sin(t*0.8)*3.0)) * 5.0 - t*1.7);
                        float c3  = sin(length(wxz - vec2(cos(t*0.5)*2.5, sin(t*0.9)*2.5)) * 5.0 - t*2.3);
                        float caustic = max(0.0, (c1 + c2 + c3) / 3.0) * 0.5;
                        color += vec3(caustic) * directionalLight * (1.0 - shadow) * 0.4 * causticFade;
                    }

                    float fog_factor = clamp((fragDist - fog_start) / (fog_end - fog_start), 0.0, 1.0);
                    float alpha = mix(texSample.a, 1.0, fog_factor);
                    FragColor = vec4(mix(color, fogColor, fog_factor), alpha);
                }"#
            )?;

            let shader = gl::CreateProgram();
            gl::AttachShader(shader, vertex_shader);
            gl::AttachShader(shader, fragment_shader);
            gl::LinkProgram(shader);

            let mut success = 0;
            gl::GetProgramiv(shader, gl::LINK_STATUS, &mut success);
            if success == 0 {
                let mut len = 0;
                gl::GetProgramiv(shader, gl::INFO_LOG_LENGTH, &mut len);
                let mut buffer = vec![0u8; len as usize];
                gl::GetProgramInfoLog(shader, len, ptr::null_mut(), buffer.as_mut_ptr() as *mut GLchar);
                return Err(format!("Shader linking failed: {}", String::from_utf8_lossy(&buffer)));
            }

            gl::DeleteShader(vertex_shader);
            gl::DeleteShader(fragment_shader);

            let model_loc            = gl::GetUniformLocation(shader, c"model".as_ptr());
            let view_loc             = gl::GetUniformLocation(shader, c"view".as_ptr());
            let projection_loc       = gl::GetUniformLocation(shader, c"projection".as_ptr());
            let use_textures_loc     = gl::GetUniformLocation(shader, c"use_textures".as_ptr());
            let fog_start_loc        = gl::GetUniformLocation(shader, c"fog_start".as_ptr());
            let fog_end_loc          = gl::GetUniformLocation(shader, c"fog_end".as_ptr());
            let transparent_pass_loc = gl::GetUniformLocation(shader, c"transparent_pass".as_ptr());
            let light_space_loc      = gl::GetUniformLocation(shader, c"lightSpaceMatrices[0]".as_ptr());
            let shadow_maps_loc      = gl::GetUniformLocation(shader, c"shadowMaps".as_ptr());
            let cascade_ends_loc     = gl::GetUniformLocation(shader, c"cascadeEnds[0]".as_ptr());
            let texel_sizes_loc      = gl::GetUniformLocation(shader, c"shadowTexelSizes[0]".as_ptr());
            let light_dir_loc        = gl::GetUniformLocation(shader, c"lightDir".as_ptr());
            let fog_color_loc        = gl::GetUniformLocation(shader, c"fog_color".as_ptr());
            let fog_override_loc     = gl::GetUniformLocation(shader, c"u_fog_override".as_ptr());
            let torch_pos_loc        = gl::GetUniformLocation(shader, c"u_torch_pos".as_ptr());
            let torch_strength_loc   = gl::GetUniformLocation(shader, c"u_torch_strength".as_ptr());
            let ambient_loc          = gl::GetUniformLocation(shader, c"ambientLight".as_ptr());
            let directional_loc      = gl::GetUniformLocation(shader, c"directionalLight".as_ptr());
            let time_loc             = gl::GetUniformLocation(shader, c"u_time".as_ptr());
            let camera_pos_loc       = gl::GetUniformLocation(shader, c"u_camera_pos".as_ptr());
            let depth_sampler_loc    = gl::GetUniformLocation(shader, c"u_depth_sampler".as_ptr());
            let proj_near_loc        = gl::GetUniformLocation(shader, c"u_proj_near".as_ptr());
            let proj_far_loc         = gl::GetUniformLocation(shader, c"u_proj_far".as_ptr());
            let screen_size_loc         = gl::GetUniformLocation(shader, c"u_screen_size".as_ptr());
            let refraction_sampler_loc  = gl::GetUniformLocation(shader, c"u_refraction_sampler".as_ptr());
            let water_level_loc         = gl::GetUniformLocation(shader, c"u_water_level".as_ptr());
            let sky_sampler_loc         = gl::GetUniformLocation(shader, c"u_sky_sampler".as_ptr());

            let texture_atlas = create_block_atlas();

            // Depth texture for water depth effects (captured after opaque pass).
            let mut depth_tex = 0u32;
            gl::GenTextures(1, &mut depth_tex);
            gl::BindTexture(gl::TEXTURE_2D, depth_tex);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            // Disable shadow comparison so we get raw depth values.
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_COMPARE_MODE, gl::NONE as i32);
            gl::BindTexture(gl::TEXTURE_2D, 0);

            // Color (refraction) texture captured alongside depth.
            let mut refraction_tex = 0u32;
            gl::GenTextures(1, &mut refraction_tex);
            gl::BindTexture(gl::TEXTURE_2D, refraction_tex);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::BindTexture(gl::TEXTURE_2D, 0);

            // Sky texture: framebuffer copy taken after the sky renders, before terrain.
            // Sampled per-pixel as the fog colour so fog blends into the actual sky.
            let mut sky_tex = 0u32;
            gl::GenTextures(1, &mut sky_tex);
            gl::BindTexture(gl::TEXTURE_2D, sky_tex);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::BindTexture(gl::TEXTURE_2D, 0);

            Ok(ChunkRenderer {
                shader,
                texture_atlas,
                depth_tex,
                refraction_tex,
                sky_tex,
                uniforms: UniformLocations {
                    model: model_loc,
                    view: view_loc,
                    projection: projection_loc,
                    use_textures: use_textures_loc,
                    fog_start: fog_start_loc,
                    fog_end: fog_end_loc,
                    transparent_pass: transparent_pass_loc,
                    light_space_matrices: light_space_loc,
                    shadow_maps: shadow_maps_loc,
                    cascade_ends: cascade_ends_loc,
                    shadow_texel_sizes: texel_sizes_loc,
                    light_dir: light_dir_loc,
                    fog_color: fog_color_loc,
                    fog_override: fog_override_loc,
                    torch_pos: torch_pos_loc,
                    torch_strength: torch_strength_loc,
                    ambient_light: ambient_loc,
                    directional_light: directional_loc,
                    time: time_loc,
                    camera_pos: camera_pos_loc,
                    depth_sampler: depth_sampler_loc,
                    proj_near: proj_near_loc,
                    proj_far: proj_far_loc,
                    screen_size: screen_size_loc,
                    refraction_sampler: refraction_sampler_loc,
                    water_level: water_level_loc,
                    sky_sampler: sky_sampler_loc,
                },
            })
        }
    }

    pub fn begin_frame(
        &self,
        view: &glam::Mat4,
        projection: &glam::Mat4,
        shadow_texture_array: u32,
        light_space_matrices: &[glam::Mat4; NUM_CASCADES],
        cascade_ends: &[f32; NUM_CASCADES],
        shadow_texel_sizes: &[f32; NUM_CASCADES],
        sun_dir: glam::Vec3,
        fog_color: glam::Vec3,
        fog_override: f32,
        torch_pos: glam::Vec3,
        torch_strength: f32,
        ambient_light: f32,
        directional_light: f32,
        fog_start: f32,
        fog_end: f32,
        elapsed_time: f32,
        camera_pos: glam::Vec3,
        proj_near: f32,
        proj_far: f32,
        screen_width: i32,
        screen_height: i32,
        water_level: f32,
    ) {
        unsafe {
            gl::UseProgram(self.shader);
            gl::UniformMatrix4fv(self.uniforms.view, 1, gl::FALSE, view.as_ref().as_ptr());
            gl::UniformMatrix4fv(self.uniforms.projection, 1, gl::FALSE, projection.as_ref().as_ptr());
            // Mat4 is repr(C) over 16 contiguous f32s, so the array is tightly
            // packed and we can upload all NUM_CASCADES at once.
            gl::UniformMatrix4fv(
                self.uniforms.light_space_matrices,
                NUM_CASCADES as i32,
                gl::FALSE,
                light_space_matrices.as_ptr() as *const f32,
            );
            gl::Uniform1fv(self.uniforms.cascade_ends, NUM_CASCADES as i32, cascade_ends.as_ptr());
            gl::Uniform1fv(self.uniforms.shadow_texel_sizes, NUM_CASCADES as i32, shadow_texel_sizes.as_ptr());
            gl::Uniform1i(self.uniforms.use_textures, 1);
            gl::Uniform1f(self.uniforms.fog_start, fog_start);
            gl::Uniform1f(self.uniforms.fog_end,   fog_end);
            gl::Uniform3f(self.uniforms.fog_color, fog_color.x, fog_color.y, fog_color.z);
            gl::Uniform1f(self.uniforms.fog_override, fog_override);
            gl::Uniform3f(self.uniforms.torch_pos, torch_pos.x, torch_pos.y, torch_pos.z);
            gl::Uniform1f(self.uniforms.torch_strength, torch_strength);
            gl::Uniform1f(self.uniforms.ambient_light, ambient_light);
            gl::Uniform1f(self.uniforms.directional_light, directional_light);
            // Texture unit 0: block atlas (sampler2D).
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.texture_atlas);
            // Texture unit 1: cascaded shadow maps (sampler2DArray).
            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D_ARRAY, shadow_texture_array);
            gl::Uniform1i(self.uniforms.shadow_maps, 1);
            gl::Uniform3f(self.uniforms.light_dir, sun_dir.x, sun_dir.y, sun_dir.z);
            gl::Uniform1f(self.uniforms.time, elapsed_time);
            gl::Uniform3f(self.uniforms.camera_pos, camera_pos.x, camera_pos.y, camera_pos.z);
            gl::Uniform1f(self.uniforms.proj_near, proj_near);
            gl::Uniform1f(self.uniforms.proj_far, proj_far);
            gl::Uniform2f(self.uniforms.screen_size, screen_width as f32, screen_height as f32);
            gl::Uniform1i(self.uniforms.depth_sampler, 2);
            gl::Uniform1i(self.uniforms.refraction_sampler, 3);
            gl::Uniform1f(self.uniforms.water_level, water_level);
            gl::Uniform1i(self.uniforms.sky_sampler, 4);
            gl::ActiveTexture(gl::TEXTURE2);
            gl::BindTexture(gl::TEXTURE_2D, self.depth_tex);
            gl::ActiveTexture(gl::TEXTURE3);
            gl::BindTexture(gl::TEXTURE_2D, self.refraction_tex);
            // Unit 4: sky texture, populated by capture_sky() called right after begin_frame.
            gl::ActiveTexture(gl::TEXTURE4);
            gl::BindTexture(gl::TEXTURE_2D, self.sky_tex);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::Enable(gl::DEPTH_TEST);
            gl::Enable(gl::CULL_FACE);
            gl::CullFace(gl::BACK);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        }
    }

    /// Copy the sky framebuffer into sky_tex (unit 4) for per-pixel fog colour.
    /// Call this after begin_frame() but before world.draw_opaque().
    pub fn capture_sky(&self, width: i32, height: i32) {
        unsafe {
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, 0);
            gl::ActiveTexture(gl::TEXTURE4);
            gl::BindTexture(gl::TEXTURE_2D, self.sky_tex);
            gl::CopyTexImage2D(gl::TEXTURE_2D, 0, gl::RGB8, 0, 0, width, height, 0);
            gl::ActiveTexture(gl::TEXTURE0);
        }
    }

    /// Copy the depth and color buffers after the opaque pass for water effects.
    pub fn capture_scene(&self, width: i32, height: i32) {
        unsafe {
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, 0);
            gl::ActiveTexture(gl::TEXTURE2);
            gl::BindTexture(gl::TEXTURE_2D, self.depth_tex);
            gl::CopyTexImage2D(gl::TEXTURE_2D, 0, gl::DEPTH_COMPONENT24, 0, 0, width, height, 0);
            gl::ActiveTexture(gl::TEXTURE3);
            gl::BindTexture(gl::TEXTURE_2D, self.refraction_tex);
            gl::CopyTexImage2D(gl::TEXTURE_2D, 0, gl::RGB8, 0, 0, width, height, 0);
            gl::ActiveTexture(gl::TEXTURE0);
        }
    }

    pub fn sky_texture(&self) -> u32 { self.sky_tex }

    pub fn set_transparent_pass(&self, enabled: bool) {
        unsafe {
            // Rebind the chunk shader and block atlas in case another renderer
            // changed the active program or TEXTURE0 between opaque and transparent.
            gl::UseProgram(self.shader);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.texture_atlas);
            gl::Uniform1i(self.uniforms.transparent_pass, enabled as i32);
            if enabled {
                gl::DepthMask(gl::FALSE);   // don't write depth for water
                gl::Disable(gl::CULL_FACE); // show water faces from inside too
            } else {
                gl::DepthMask(gl::TRUE);
                gl::Enable(gl::CULL_FACE);
                gl::CullFace(gl::BACK);
            }
        }
    }

    pub fn texture_atlas(&self) -> u32 {
        self.texture_atlas
    }

    pub fn end_frame(&self) {
        unsafe {
            gl::DepthMask(gl::TRUE);
            gl::Disable(gl::BLEND);
            gl::BindVertexArray(0);
            gl::UseProgram(0);
        }
    }

    pub fn draw_chunk(&self, chunk: &Chunk) {
        let mesh = match &chunk.mesh {
            Some(m) => m,
            None => return,
        };

        unsafe {
            let model = chunk.model_matrix();
            gl::UniformMatrix4fv(self.uniforms.model, 1, gl::FALSE, model.as_ref().as_ptr());
            gl::BindVertexArray(mesh.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, mesh.vertex_count);
        }
    }
}

impl Drop for ChunkRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.shader);
            gl::DeleteTextures(1, &self.texture_atlas);
            gl::DeleteTextures(1, &self.depth_tex);
            gl::DeleteTextures(1, &self.refraction_tex);
            gl::DeleteTextures(1, &self.sky_tex);
        }
    }
}
