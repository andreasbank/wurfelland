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

    pub fn get_view_matrix(&self) -> glam::Mat4 {
        let center = [
            self.position[0] + self.front[0],
            self.position[1] + self.front[1],
            self.position[2] + self.front[2],
        ];
        glam::Mat4::look_at_rh(
            glam::Vec3::from(self.position),
            glam::Vec3::from(center),
            glam::Vec3::from(self.up),
        )
    }
    
    pub fn move_to_rel(&mut self, x: f32, y: f32, z: f32) {
        self.position[0] += x;
        self.position[1] += y;
        self.position[2] += z;
    }

    pub fn move_to_abs(&mut self, x: f32, y: f32, z: f32) {
        self.position[0] = x;
        self.position[1] = y;
        self.position[2] = z;
    }

    pub fn update_pitch_yaw(&mut self, pitch: f32, yaw: f32) {
        self.pitch = pitch;
        self.yaw = yaw;
        self.front = [
            self.yaw.to_radians().cos() * self.pitch.to_radians().cos(),
            self.pitch.to_radians().sin(),
            self.yaw.to_radians().sin() * self.pitch.to_radians().cos(),
        ];
    }

    //pub fn process_movement(&mut self,
    //                        forward: bool,
    //                        back: bool,
    //                        left: bool,
    //                        right: bool,
    //                        jump: bool) {
    //    if forward {
    //        self.position[0] += self.move_speed * self.yaw.to_radians().cos();
    //        self.position[2] += self.move_speed * self.yaw.to_radians().sin();              
    //    }
    //    if back {
    //        self.position[0] -= self.move_speed * self.yaw.to_radians().cos();
    //        self.position[2] -= self.move_speed * self.yaw.to_radians().sin();
    //    }
    //    if left {
    //        self.position[0] -= self.move_speed * (self.yaw + 90.0).to_radians().cos();
    //        self.position[2] -= self.move_speed * (self.yaw + 90.0).to_radians().sin();
    //    }
    //    if right {
    //        self.position[0] += self.move_speed * (self.yaw + 90.0).to_radians().cos();
    //        self.position[2] += self.move_speed * (self.yaw + 90.0).to_radians().sin();
    //    } 
    //    if jump {
    //        if self.jumping == false {
    //            self.jumping = true;
    //        }
    //        self.position[1] += self.move_speed;
    //    } else {
    //        if self.jumping {
    //            self.position[1] -= self.move_speed;
    //            if self.position[1] <= 0.0 {
    //                self.position[1] = 0.0;
    //                self.jumping = false;
    //            }
    //        }
    //    }
    //}
}