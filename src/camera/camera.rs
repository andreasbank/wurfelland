use glam::{Mat4, Vec3};

use crate::camera::frustum::Frustum;

pub struct Camera {
    pub position: Vec3,
    pub front: Vec3,
    pub up: Vec3,
    pub fov: f32,
    pub aspect_ratio: f32,
    pub near_plane: f32,
    pub far_plane: f32,
    // For mouse control:
    pub yaw: f32,
    pub pitch: f32,
}

impl Camera {
    pub fn new(width: u32, height: u32) -> Self {
        Camera {
            position: Vec3::new(0.0, 0.0, 3.0),
            front: Vec3::new(0.0, 0.0, -1.0),
            up: Vec3::new(0.0, 1.0, 0.0),
            fov: 45.0,
            aspect_ratio: width as f32 / height as f32,
            near_plane: 0.1,
            far_plane: 1000.0,
            yaw: -90.0,
            pitch: 0.0,
        }
    }
    
    pub fn update_pitch_yaw(&mut self, pitch: f32, yaw: f32) {
        self.pitch = pitch;
        self.yaw = yaw;
        let front = Vec3::new(
            yaw.to_radians().cos() * pitch.to_radians().cos(),
            pitch.to_radians().sin(),
            yaw.to_radians().sin() * pitch.to_radians().cos(),
        );
        self.front = front.normalize();
    }

    pub fn move_to_abs(&mut self, x: f32, y: f32, z: f32) {
        self.position = Vec3::new(x, y, z);
    }

    pub fn on_resize(&mut self, width: u32, height: u32) {
        self.aspect_ratio = width as f32 / height as f32;
    }

    pub fn view_matrix(&self) -> Mat4 {
        // Using look_at_rh (right-handed coordinate system)
        // Eye = camera position
        // Center = where camera is looking (position + front direction)
        // Up = world up vector (usually (0,1,0))
        Mat4::look_at_rh(
            self.position,               // eye
            self.position + self.front,  // center (look-at point)
            self.up,                     // up
        )
    }

    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(
            self.fov.to_radians(),  // Vertical field of view in radians
            self.aspect_ratio,      // Width / height (e.g., 800/600 = 1.333)
            self.near_plane,        // Near clipping plane (e.g., 0.1)
            self.far_plane,         // Far clipping plane (e.g., 1000.0)
        )
    }
    
    pub fn frustum(&self) -> Frustum {
        Frustum::from_view_projection(&self.view_matrix(), &self.projection_matrix())
    }
}