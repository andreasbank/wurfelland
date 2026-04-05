use glam::{Vec3, Vec4, Mat4};

pub struct Frustum {
    planes: [Vec4; 6], // left, right, bottom, top, near, far
}

impl Frustum {
    pub fn from_view_projection(view: &Mat4, projection: &Mat4) -> Self {
        let mvp = projection * view;
        let mut planes = [Vec4::ZERO; 6];
        
        // Extract frustum planes from MVP matrix
        // Left
        planes[0] = mvp.row(3) + mvp.row(0);
        // Right
        planes[1] = mvp.row(3) - mvp.row(0);
        // Bottom
        planes[2] = mvp.row(3) + mvp.row(1);
        // Top
        planes[3] = mvp.row(3) - mvp.row(1);
        // Near
        planes[4] = mvp.row(3) + mvp.row(2);
        // Far
        planes[5] = mvp.row(3) - mvp.row(2);
        
        // Normalize planes
        for plane in &mut planes {
            let length = Vec3::new(plane.x, plane.y, plane.z).length();
            *plane = *plane / length;
        }
        
        Frustum { planes }
    }
    
    pub fn intersects_aabb(&self, min: Vec3, max: Vec3) -> bool {
        for i in 0..6 {
            let plane = self.planes[i];
            
            // Find the AABB vertex that's furthest along the plane normal
            let nx = if plane.x > 0.0 { max.x } else { min.x };
            let ny = if plane.y > 0.0 { max.y } else { min.y };
            let nz = if plane.z > 0.0 { max.z } else { min.z };
            
            // If even the furthest vertex is behind the plane, AABB is outside
            if plane.x * nx + plane.y * ny + plane.z * nz + plane.w < 0.0 {
                return false;
            }
        }
        true
    }
}