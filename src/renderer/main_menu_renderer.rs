use crate::renderer::ui::{UiRenderer, TextButton, create_text_texture_scaled, TextTexture};

pub struct MainMenuRenderer {
    renderer: UiRenderer,
    title: TextTexture,
    loading_label: TextTexture,
    buttons: Vec<TextButton>,
}

impl MainMenuRenderer {
    pub fn new() -> Self {
        let renderer = UiRenderer::new();
        let title   = create_text_texture_scaled("WURFELLAND", 4);
        let loading_label = create_text_texture_scaled("LOADING", 3);
        let buttons = vec![
            TextButton::new("singleplayer", "SINGLEPLAYER", (0.30, 0.39, 0.70, 0.46)),
            TextButton::new("load_game",    "LOAD GAME",    (0.30, 0.48, 0.70, 0.55)),
            TextButton::new("multiplayer",  "MULTIPLAYER",  (0.30, 0.57, 0.70, 0.64)),
            TextButton::new("options",      "OPTIONS",      (0.30, 0.66, 0.70, 0.73)),
            TextButton::new("credits",      "CREDITS",      (0.30, 0.79, 0.70, 0.86)),
            TextButton::new("exit",         "QUIT GAME",    (0.30, 0.88, 0.70, 0.96)),
        ];
        MainMenuRenderer { renderer, title, loading_label, buttons }
    }

    /// Draw the main menu over the world background.
    /// `ready` — whether the background world has finished loading; buttons are
    /// greyed-out and non-interactive until it is.
    pub fn draw(&self, progress: f32, ready: bool, win_w: f32, win_h: f32) {
        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        }

        // Semi-transparent dark panel behind the buttons
        self.renderer.draw_rect(0.28, 0.32, 0.72, 0.99, 0.0, 0.0, 0.0, 0.58);

        // Title
        let tw = self.title.pixel_width  as f32 / win_w;
        let th = self.title.pixel_height as f32 / win_h;
        self.renderer.draw_text(&self.title,
            0.5 - tw * 0.5, 0.25 - th * 0.5,
            0.5 + tw * 0.5, 0.25 + th * 0.5);

        for btn in &self.buttons {
            btn.draw(&self.renderer, win_w, win_h);
        }

        // Dim the game/options buttons while the world is still loading
        if !ready {
            self.renderer.draw_rect(0.28, 0.37, 0.72, 0.75, 0.0, 0.0, 0.0, 0.50);
        }

        // World-loading progress bar — hidden once fully loaded
        if !ready {
            self.draw_bar(0.32, 0.84, 0.68, 0.87, progress);
        }

        unsafe {
            gl::Enable(gl::DEPTH_TEST);
            gl::Disable(gl::BLEND);
        }
    }

    /// Full-screen loading overlay used during the game-start load phase.
    pub fn draw_loading_screen(&self, progress: f32, win_w: f32, win_h: f32) {
        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::CULL_FACE);
            gl::PolygonMode(gl::FRONT_AND_BACK, gl::FILL);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        }

        // Solid background — same color regardless of what was rendered behind it
        self.renderer.draw_rect(0.0, 0.0, 1.0, 1.0, 0.07, 0.10, 0.16, 1.0);

        let lw = self.loading_label.pixel_width  as f32 / win_w;
        let lh = self.loading_label.pixel_height as f32 / win_h;
        self.renderer.draw_text(&self.loading_label,
            0.5 - lw * 0.5, 0.44 - lh * 0.5,
            0.5 + lw * 0.5, 0.44 + lh * 0.5);

        self.draw_bar(0.20, 0.55, 0.80, 0.61, progress);

        unsafe {
            gl::Enable(gl::DEPTH_TEST);
            gl::Disable(gl::BLEND);
        }
    }

    fn draw_bar(&self, x0: f32, y0: f32, x1: f32, y1: f32, progress: f32) {
        self.renderer.draw_rect(x0, y0, x1, y1, 0.22, 0.22, 0.22, 0.90);
        let fill = x0 + (x1 - x0) * progress.clamp(0.0, 1.0);
        if fill > x0 {
            self.renderer.draw_rect(x0, y0, fill, y1, 0.20, 0.68, 0.28, 1.0);
        }
    }

    pub fn handle_click(&self, mx: f32, my: f32, win_w: f32, win_h: f32, ready: bool) -> Option<&str> {
        let nx = mx / win_w;
        let ny = my / win_h;
        let hit = self.buttons.iter().find(|b| b.is_hit(nx, ny)).map(|b| b.id.as_str());
        if !ready && hit != Some("exit") && hit != Some("credits") { return None; }
        hit
    }
}
