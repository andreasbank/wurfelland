use std::mem;
use std::os::raw::c_void;

// Interleaved vertex layout: [x, y, z,  nx, ny, nz,  u, v] = 8 floats
pub const GLTF_STRIDE: usize = 8;

pub struct GltfModel {
    pub vao:         u32,
    pub vbo:         u32,
    pub ebo:         u32,
    pub index_count: i32,
    pub texture:     u32,
}

// Walk the node hierarchy, accumulating transforms, collecting all primitive data.
fn collect_node(
    node:        gltf::Node<'_>,
    parent_xf:   glam::Mat4,
    buffers:     &[gltf::buffer::Data],
    positions:   &mut Vec<[f32; 3]>,
    normals:     &mut Vec<[f32; 3]>,
    uvs:         &mut Vec<[f32; 2]>,
    indices:     &mut Vec<u32>,
) {
    let local: [[f32; 4]; 4] = node.transform().matrix();
    let world = parent_xf * glam::Mat4::from_cols_array_2d(&local);

    if let Some(mesh) = node.mesh() {
        for prim in mesh.primitives() {
            let base = positions.len() as u32;
            let reader = prim.reader(|buf| Some(&buffers[buf.index()]));

            let pos_start = positions.len();

            if let Some(iter) = reader.read_positions() {
                for p in iter {
                    let wp = world.transform_point3(glam::Vec3::from(p));
                    positions.push(wp.to_array());
                }
            }

            let vert_count = positions.len() - pos_start;

            if let Some(iter) = reader.read_normals() {
                for n in iter {
                    // For rigid-body transforms (translation + rotation only),
                    // mat3(world) is the correct normal matrix.
                    let wn = (glam::Mat3::from_mat4(world) * glam::Vec3::from(n))
                        .normalize_or(glam::Vec3::Y);
                    normals.push(wn.to_array());
                }
            } else {
                for _ in 0..vert_count {
                    normals.push([0.0, 1.0, 0.0]);
                }
            }

            if let Some(iter) = reader.read_tex_coords(0) {
                for uv in iter.into_f32() {
                    uvs.push(uv);
                }
            } else {
                for _ in 0..vert_count {
                    uvs.push([0.0, 0.0]);
                }
            }

            if let Some(iter) = reader.read_indices() {
                for idx in iter.into_u32() {
                    indices.push(base + idx);
                }
            }
        }
    }

    for child in node.children() {
        collect_node(child, world, buffers, positions, normals, uvs, indices);
    }
}

impl GltfModel {
    pub fn load(path: &str) -> Result<Self, String> {
        let (doc, buffers, images) = gltf::import(path).map_err(|e| e.to_string())?;

        let mut positions: Vec<[f32; 3]> = Vec::new();
        let mut normals:   Vec<[f32; 3]> = Vec::new();
        let mut uvs:       Vec<[f32; 2]> = Vec::new();
        let mut indices:   Vec<u32>      = Vec::new();

        for scene in doc.scenes() {
            for node in scene.nodes() {
                collect_node(node, glam::Mat4::IDENTITY, &buffers,
                    &mut positions, &mut normals, &mut uvs, &mut indices);
            }
        }

        if positions.is_empty() {
            return Err("no geometry in GLTF".to_string());
        }

        // Center the model on XZ, floor the lowest Y to 0.
        let min_y = positions.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        let min_x = positions.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
        let max_x = positions.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
        let min_z = positions.iter().map(|p| p[2]).fold(f32::MAX, f32::min);
        let max_z = positions.iter().map(|p| p[2]).fold(f32::MIN, f32::max);
        let cx = (min_x + max_x) * 0.5;
        let cz = (min_z + max_z) * 0.5;
        for p in &mut positions {
            p[0] -= cx;
            p[1] -= min_y;
            p[2] -= cz;
        }

        // Build interleaved vertex buffer.
        let mut verts: Vec<f32> = Vec::with_capacity(positions.len() * GLTF_STRIDE);
        for i in 0..positions.len() {
            verts.extend_from_slice(&positions[i]);
            verts.extend_from_slice(&normals[i]);
            verts.extend_from_slice(&uvs[i]);
        }

        let (vao, vbo, ebo) = upload_vao(&verts, &indices);
        let texture = if images.is_empty() {
            create_white_texture()
        } else {
            load_texture(&images[0])?
        };

        Ok(GltfModel { vao, vbo, ebo, index_count: indices.len() as i32, texture })
    }
}

fn upload_vao(verts: &[f32], indices: &[u32]) -> (u32, u32, u32) {
    let mut vao = 0u32;
    let mut vbo = 0u32;
    let mut ebo = 0u32;
    let stride = (GLTF_STRIDE * mem::size_of::<f32>()) as i32;
    unsafe {
        gl::GenVertexArrays(1, &mut vao);
        gl::GenBuffers(1, &mut vbo);
        gl::GenBuffers(1, &mut ebo);
        gl::BindVertexArray(vao);

        gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
        gl::BufferData(gl::ARRAY_BUFFER,
            (verts.len() * mem::size_of::<f32>()) as isize,
            verts.as_ptr() as *const c_void,
            gl::STATIC_DRAW);

        gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, ebo);
        gl::BufferData(gl::ELEMENT_ARRAY_BUFFER,
            (indices.len() * mem::size_of::<u32>()) as isize,
            indices.as_ptr() as *const c_void,
            gl::STATIC_DRAW);

        // attrib 0: position (vec3)
        gl::VertexAttribPointer(0, 3, gl::FLOAT, gl::FALSE, stride, std::ptr::null());
        gl::EnableVertexAttribArray(0);
        // attrib 1: normal (vec3)
        gl::VertexAttribPointer(1, 3, gl::FLOAT, gl::FALSE, stride,
            (3 * mem::size_of::<f32>()) as *const c_void);
        gl::EnableVertexAttribArray(1);
        // attrib 2: uv (vec2)
        gl::VertexAttribPointer(2, 2, gl::FLOAT, gl::FALSE, stride,
            (6 * mem::size_of::<f32>()) as *const c_void);
        gl::EnableVertexAttribArray(2);

        gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        gl::BindVertexArray(0);
    }
    (vao, vbo, ebo)
}

fn load_texture(img: &gltf::image::Data) -> Result<u32, String> {
    use gltf::image::Format;
    let pixels: Vec<u8> = match img.format {
        Format::R8G8B8 => img.pixels.chunks(3)
            .flat_map(|c| [c[0], c[1], c[2], 255u8])
            .collect(),
        Format::R8G8B8A8 => img.pixels.clone(),
        other => return Err(format!("unsupported GLTF image format {:?}", other)),
    };

    let mut tex = 0u32;
    unsafe {
        gl::GenTextures(1, &mut tex);
        gl::BindTexture(gl::TEXTURE_2D, tex);
        gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as i32,
            img.width as i32, img.height as i32, 0,
            gl::RGBA, gl::UNSIGNED_BYTE,
            pixels.as_ptr() as *const c_void);
        gl::GenerateMipmap(gl::TEXTURE_2D);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR_MIPMAP_LINEAR as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
        gl::BindTexture(gl::TEXTURE_2D, 0);
    }
    Ok(tex)
}

fn create_white_texture() -> u32 {
    let mut tex = 0u32;
    let pixels = [255u8, 255, 255, 255];
    unsafe {
        gl::GenTextures(1, &mut tex);
        gl::BindTexture(gl::TEXTURE_2D, tex);
        gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as i32, 1, 1, 0,
            gl::RGBA, gl::UNSIGNED_BYTE,
            pixels.as_ptr() as *const c_void);
        gl::BindTexture(gl::TEXTURE_2D, 0);
    }
    tex
}

impl Drop for GltfModel {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteBuffers(1, &self.ebo);
            gl::DeleteTextures(1, &self.texture);
        }
    }
}
