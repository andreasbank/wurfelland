pub struct Camera {
    pub position: [f32; 3],
    pub front: [f32; 3],
    pub up: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub mouse_sensitivity: f32,
    pub move_speed: f32,
}

impl Camera {
    pub fn new() -> Self {
        Camera {
            position: [0.0, 0.0, 3.0],
            front: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            yaw: -90.0,
            pitch: 0.0,
            mouse_sensitivity: 0.1,
            move_speed: 0.1,
        }
    }
    
    pub fn process_mouse_movement(&mut self, xoffset: f32, yoffset: f32) {
        self.yaw += xoffset * self.mouse_sensitivity;
        self.pitch += yoffset * self.mouse_sensitivity;

        // Clamp pitch
        if self.pitch > 89.0 { self.pitch = 89.0; }
        if self.pitch < -89.0 { self.pitch = -89.0; }
        self.front = [
            self.yaw.to_radians().cos() * self.pitch.to_radians().cos(),
            self.pitch.to_radians().sin(),
            self.yaw.to_radians().sin() * self.pitch.to_radians().cos(),
        ];
    }

    pub fn process_movement(&mut self, forward: bool, back: bool, left: bool, right: bool) {
        if forward {
            self.position[0] += self.move_speed * self.yaw.to_radians().cos();
            self.position[2] += self.move_speed * self.yaw.to_radians().sin();              
        }
        if back {
            self.position[0] -= self.move_speed * self.yaw.to_radians().cos();
            self.position[2] -= self.move_speed * self.yaw.to_radians().sin();
        }
        if left {
            self.position[0] -= self.move_speed * (self.yaw + 90.0).to_radians().cos();
            self.position[2] -= self.move_speed * (self.yaw + 90.0).to_radians().sin();
        }
        if right {
            self.position[0] += self.move_speed * (self.yaw + 90.0).to_radians().cos();
            self.position[2] += self.move_speed * (self.yaw + 90.0).to_radians().sin();
        } 
    }
}