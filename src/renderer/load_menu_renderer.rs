use crate::renderer::ui::{UiRenderer, TextButton, TextTexture, create_text_texture_scaled};

const MAX_VISIBLE: usize = 6;

pub struct LoadMenuRenderer {
    renderer:  UiRenderer,
    title:     TextTexture,
    empty_tex: TextTexture,
    back_btn:  TextButton,
    save_btns: Vec<TextButton>,
}

impl LoadMenuRenderer {
    pub fn new() -> Self {
        let renderer  = UiRenderer::new();
        let title     = create_text_texture_scaled("LOAD GAME", 3);
        let empty_tex = create_text_texture_scaled("NO SAVES", 2);
        let back_btn  = TextButton::new("back", "BACK", (0.30, 0.82, 0.70, 0.90));
        LoadMenuRenderer { renderer, title, empty_tex, back_btn, save_btns: Vec::new() }
    }

    /// Rebuild the save slot buttons from the current list of save names.
    /// Call this each time the load menu is opened.
    pub fn refresh(&mut self, saves: &[String]) {
        self.save_btns.clear();
        for (i, name) in saves.iter().take(MAX_VISIBLE).enumerate() {
            let y0    = 0.28 + i as f32 * 0.09;
            let y1    = y0 + 0.08;
            let label = format!("SAVE-{}", name);
            self.save_btns.push(TextButton::new(name, &label, (0.30, y0, 0.70, y1)));
        }
    }

    pub fn draw(&self, win_w: f32, win_h: f32) {
        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        }

        self.renderer.draw_rect(0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.65);
        self.renderer.draw_rect(0.25, 0.10, 0.75, 0.94, 0.10, 0.10, 0.14, 0.90);

        let tw = self.title.pixel_width  as f32 / win_w;
        let th = self.title.pixel_height as f32 / win_h;
        self.renderer.draw_text(&self.title,
            0.5 - tw * 0.5, 0.16 - th * 0.5,
            0.5 + tw * 0.5, 0.16 + th * 0.5);

        if self.save_btns.is_empty() {
            let ew = self.empty_tex.pixel_width  as f32 / win_w;
            let eh = self.empty_tex.pixel_height as f32 / win_h;
            self.renderer.draw_text(&self.empty_tex,
                0.5 - ew * 0.5, 0.48 - eh * 0.5,
                0.5 + ew * 0.5, 0.48 + eh * 0.5);
        } else {
            for btn in &self.save_btns {
                btn.draw(&self.renderer, win_w, win_h);
            }
        }

        self.back_btn.draw(&self.renderer, win_w, win_h);

        unsafe {
            gl::Enable(gl::DEPTH_TEST);
            gl::Disable(gl::BLEND);
        }
    }

    /// Returns `"back"` if the back button was clicked, or the save name if a
    /// slot was clicked, otherwise `None`.
    pub fn handle_click(&self, mx: f32, my: f32, win_w: f32, win_h: f32) -> Option<&str> {
        let nx = mx / win_w;
        let ny = my / win_h;
        if self.back_btn.is_hit(nx, ny) { return Some("back"); }
        self.save_btns.iter()
            .find(|b| b.is_hit(nx, ny))
            .map(|b| b.id.as_str())
    }
}
