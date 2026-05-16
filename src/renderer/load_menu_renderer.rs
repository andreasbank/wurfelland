use crate::renderer::ui::{UiRenderer, TextButton, TextTexture, create_text_texture_scaled, load_image_texture};

const MAX_VISIBLE: usize = 6;

// Gap between the save button and the trash button (normalised screen units).
const TRASH_GAP:   f32 = 0.01;
// Save buttons now end at 0.64 so the trash button fits inside the panel (0.25–0.75).
const SAVE_X0: f32 = 0.30;
const SAVE_X1: f32 = 0.64;
const TRASH_X0: f32 = SAVE_X1 + TRASH_GAP;
const TRASH_X1: f32 = 0.74;

pub enum LoadMenuAction {
    Back,
    Load(String),
    Delete(String),
}

pub struct LoadMenuRenderer {
    renderer:     UiRenderer,
    title:        TextTexture,
    empty_tex:    TextTexture,
    back_btn:     TextButton,
    save_btns:    Vec<TextButton>,
    trash_btns:   Vec<(String, f32, f32, f32, f32)>,  // (save_name, x0, y0, x1, y1)
    trashcan_tex: TextTexture,
}

impl LoadMenuRenderer {
    pub fn new() -> Self {
        let renderer     = UiRenderer::new();
        let title        = create_text_texture_scaled("LOAD GAME", 3);
        let empty_tex    = create_text_texture_scaled("NO SAVES", 2);
        let back_btn     = TextButton::new("back", "BACK", (0.30, 0.82, 0.70, 0.90));
        let trashcan_tex = load_image_texture("assets/ui/trashcan.png");
        LoadMenuRenderer {
            renderer, title, empty_tex, back_btn,
            save_btns: Vec::new(), trash_btns: Vec::new(),
            trashcan_tex,
        }
    }

    /// Rebuild save slot and trash buttons from the current list of save names.
    pub fn refresh(&mut self, saves: &[String]) {
        self.save_btns.clear();
        self.trash_btns.clear();
        for (i, name) in saves.iter().take(MAX_VISIBLE).enumerate() {
            let y0 = 0.28 + i as f32 * 0.09;
            let y1 = y0 + 0.08;
            self.save_btns.push(TextButton::new(
                name,
                &format!("SAVE-{}", name),
                (SAVE_X0, y0, SAVE_X1, y1),
            ));
            self.trash_btns.push((name.clone(), TRASH_X0, y0, TRASH_X1, y1));
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
            for &(_, x0, y0, x1, y1) in &self.trash_btns {
                let pad = 0.01;
                self.renderer.draw_rect(x0 - pad, y0 - pad, x1 + pad, y1 + pad, 0.8, 0.8, 0.8, 1.0);
                self.renderer.draw_rect(x0, y0, x1, y1, 0.55, 0.15, 0.15, 1.0);

                // Draw icon as a square centred inside the button.
                let btn_cx = (x0 + x1) / 2.0;
                let btn_cy = (y0 + y1) / 2.0;
                let icon_h = (y1 - y0) * 0.75;
                let icon_w = icon_h * (win_h / win_w);
                self.renderer.draw_text(
                    &self.trashcan_tex,
                    btn_cx - icon_w / 2.0, btn_cy - icon_h / 2.0,
                    btn_cx + icon_w / 2.0, btn_cy + icon_h / 2.0,
                );
            }
        }

        self.back_btn.draw(&self.renderer, win_w, win_h);

        unsafe {
            gl::Enable(gl::DEPTH_TEST);
            gl::Disable(gl::BLEND);
        }
    }

    pub fn handle_click(&self, mx: f32, my: f32, win_w: f32, win_h: f32) -> Option<LoadMenuAction> {
        let nx = mx / win_w;
        let ny = my / win_h;
        if self.back_btn.is_hit(nx, ny) { return Some(LoadMenuAction::Back); }
        for (name, x0, y0, x1, y1) in &self.trash_btns {
            if nx >= *x0 && nx <= *x1 && ny >= *y0 && ny <= *y1 {
                return Some(LoadMenuAction::Delete(name.clone()));
            }
        }
        self.save_btns.iter()
            .find(|b| b.is_hit(nx, ny))
            .map(|b| LoadMenuAction::Load(b.id.clone()))
    }
}
