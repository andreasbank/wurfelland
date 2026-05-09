use crate::renderer::ui::{UiRenderer, TextButton, create_text_texture_scaled, TextTexture};

pub struct MultiplayerMenuRenderer {
    renderer: UiRenderer,
    title: TextTexture,
    ip_label: TextTexture,
    ip_texture: Option<TextTexture>,
    ip_cached: String,
    buttons_main: Vec<TextButton>,   // HOST, JOIN, BACK
    buttons_join: Vec<TextButton>,   // CONNECT, BACK
    pub join_mode: bool,
}

impl MultiplayerMenuRenderer {
    pub fn new() -> Self {
        let renderer = UiRenderer::new();
        let title    = create_text_texture_scaled("MULTIPLAYER", 3);
        let ip_label = create_text_texture_scaled("IP:", 2);

        let buttons_main = vec![
            TextButton::new("host", "HOST", (0.28, 0.44, 0.50, 0.54)),
            TextButton::new("join", "JOIN", (0.52, 0.44, 0.74, 0.54)),
            TextButton::new("back", "BACK", (0.35, 0.72, 0.65, 0.80)),
        ];

        let buttons_join = vec![
            TextButton::new("connect", "CONNECT", (0.30, 0.60, 0.70, 0.68)),
            TextButton::new("back",    "BACK",    (0.35, 0.72, 0.65, 0.80)),
        ];

        MultiplayerMenuRenderer {
            renderer,
            title,
            ip_label,
            ip_texture: None,
            ip_cached: String::new(),
            buttons_main,
            buttons_join,
            join_mode: false,
        }
    }

    /// Rebuild the IP display texture only when the IP string changes.
    pub fn update_ip(&mut self, ip: &str) {
        if ip != self.ip_cached {
            self.ip_cached = ip.to_string();
            self.ip_texture = if ip.is_empty() {
                None
            } else {
                Some(create_text_texture_scaled(ip, 2))
            };
        }
    }

    pub fn draw(&self, win_w: f32, win_h: f32) {
        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        }

        // Dark semi-transparent panel
        self.renderer.draw_rect(0.25, 0.30, 0.75, 0.85, 0.0, 0.0, 0.0, 0.70);

        // Title "MULTIPLAYER" centered around y=0.35
        let tw = self.title.pixel_width  as f32 / win_w;
        let th = self.title.pixel_height as f32 / win_h;
        self.renderer.draw_text(
            &self.title,
            0.5 - tw * 0.5, 0.35 - th * 0.5,
            0.5 + tw * 0.5, 0.35 + th * 0.5,
        );

        if self.join_mode {
            // "IP:" label
            let lw = self.ip_label.pixel_width  as f32 / win_w;
            let lh = self.ip_label.pixel_height as f32 / win_h;
            let label_x = 0.30;
            let label_y = 0.50;
            self.renderer.draw_text(
                &self.ip_label,
                label_x, label_y - lh * 0.5,
                label_x + lw, label_y + lh * 0.5,
            );

            // IP value text (right of label)
            if let Some(ref ip_tex) = self.ip_texture {
                let iw = ip_tex.pixel_width  as f32 / win_w;
                let ih = ip_tex.pixel_height as f32 / win_h;
                let ip_x = label_x + lw + 0.01;
                self.renderer.draw_text(
                    ip_tex,
                    ip_x, label_y - ih * 0.5,
                    ip_x + iw, label_y + ih * 0.5,
                );
            }

            for btn in &self.buttons_join {
                btn.draw(&self.renderer, win_w, win_h);
            }
        } else {
            for btn in &self.buttons_main {
                btn.draw(&self.renderer, win_w, win_h);
            }
        }

        unsafe {
            gl::Enable(gl::DEPTH_TEST);
            gl::Disable(gl::BLEND);
        }
    }

    /// Returns "host", "join", "connect", or "back" depending on the active
    /// mode and which button (if any) was hit by the click.
    pub fn handle_click(&self, mx: f32, my: f32, win_w: f32, win_h: f32) -> Option<&str> {
        let nx = mx / win_w;
        let ny = my / win_h;
        let buttons = if self.join_mode { &self.buttons_join } else { &self.buttons_main };
        buttons.iter()
            .find(|b| b.is_hit(nx, ny))
            .map(|b| b.id.as_str())
    }
}
