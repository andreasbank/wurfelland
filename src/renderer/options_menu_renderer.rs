use crate::renderer::ui::{Window, TextButton};

pub struct OptionsMenuRenderer {
    window: Window,
}

impl OptionsMenuRenderer {
    pub fn new() -> Self {
        let mut window = Window::new()
            .with_overlay(0.6)
            .with_title("OPTIONS", (0.30, 0.08, 0.70, 0.18));
        window.add_button(TextButton::new_toggle(
            "fog",
            &["FOG: NEAR", "FOG: NORMAL", "FOG: FAR", "FOG: OFF"],
            (0.30, 0.21, 0.70, 0.28),
        ));
        window.add_button(TextButton::new_toggle(
            "chunks",
            &["CHUNKS: 4", "CHUNKS: 6", "CHUNKS: 8", "CHUNKS: 10", "CHUNKS: 12"],
            (0.30, 0.30, 0.70, 0.37),
        ));
        window.add_button(TextButton::new_toggle(
            "outline",
            &["OUTLINE: OFF", "OUTLINE: ON"],
            (0.30, 0.39, 0.70, 0.46),
        ));
        window.add_button(TextButton::new_toggle(
            "stats",
            &["STATS: OFF", "STATS: ON"],
            (0.30, 0.48, 0.70, 0.55),
        ));
        window.add_button(TextButton::new_toggle(
            "res",
            &["RES: LO", "RES: HI"],
            (0.30, 0.57, 0.70, 0.64),
        ));
        window.add_button(TextButton::new_toggle(
            "chunk_outlines",
            &["CHUNK OUTLINES: OFF", "CHUNK OUTLINES: ON"],
            (0.30, 0.66, 0.70, 0.73),
        ));
        window.add_button(TextButton::new_toggle(
            "entity_outlines",
            &["ENTITY OUTLINES: OFF", "ENTITY OUTLINES: ON"],
            (0.30, 0.75, 0.70, 0.82),
        ));
        window.add_button(TextButton::new("back", "BACK", (0.35, 0.86, 0.65, 0.94)));
        OptionsMenuRenderer { window }
    }

    pub fn draw(&mut self, fog_idx: usize, chunk_radius_idx: usize, outline_enabled: bool, stats_enabled: bool, hi_res: bool, chunk_outlines: bool, entity_outlines: bool, win_w: f32, win_h: f32) {
        self.window.button_mut("fog").unwrap().set_label(fog_idx);
        self.window.button_mut("chunks").unwrap().set_label(chunk_radius_idx);
        self.window.button_mut("outline").unwrap().set_label(outline_enabled as usize);
        self.window.button_mut("stats").unwrap().set_label(stats_enabled as usize);
        self.window.button_mut("res").unwrap().set_label(hi_res as usize);
        self.window.button_mut("chunk_outlines").unwrap().set_label(chunk_outlines as usize);
        self.window.button_mut("entity_outlines").unwrap().set_label(entity_outlines as usize);
        self.window.draw(win_w, win_h);
    }

    pub fn handle_click(&self, mx: f32, my: f32, win_w: f32, win_h: f32) -> Option<&str> {
        self.window.handle_click(mx / win_w, my / win_h)
    }
}
