use crate::renderer::ui::{Window, TextButton, Slider};

pub struct OptionsMenuRenderer {
    window:       Window,
    audio_window: Window,
    pub audio_open: bool,
}

fn row(n: u8) -> (f32, f32, f32, f32) {
    let y0 = 0.14 + n as f32 * 0.08;
    (0.30, y0, 0.70, y0 + 0.065)
}

impl OptionsMenuRenderer {
    pub fn new() -> Self {
        // ── Main options ───────────────────────────────────────────────────────
        let mut window = Window::new()
            .with_overlay(0.6)
            .with_title("OPTIONS", (0.28, 0.04, 0.72, 0.12));
        window.add_button(TextButton::new_toggle("fog",
            &["FOG: NEAR", "FOG: NORMAL", "FOG: FAR", "FOG: OFF"], row(0)));
        window.add_button(TextButton::new_toggle("chunks",
            &["CHUNKS: 4", "CHUNKS: 6", "CHUNKS: 8", "CHUNKS: 10", "CHUNKS: 12"], row(1)));
        window.add_button(TextButton::new_toggle("sim_radius",
            &["SIM: 2", "SIM: 4", "SIM: 6", "SIM: 8", "SIM: ALL"], row(2)));
        window.add_button(TextButton::new_toggle("outline",
            &["OUTLINE: OFF", "OUTLINE: ON"], row(3)));
        window.add_button(TextButton::new_toggle("stats",
            &["STATS: OFF", "STATS: ON"], row(4)));
        window.add_button(TextButton::new_toggle("res",
            &["RES: LO", "RES: HI"], row(5)));
        window.add_button(TextButton::new_toggle("chunk_outlines",
            &["CHUNK OUTLINES: OFF", "CHUNK OUTLINES: ON"], row(6)));
        window.add_button(TextButton::new_toggle("entity_outlines",
            &["ENTITY OUTLINES: OFF", "ENTITY OUTLINES: ON"], row(7)));
        window.add_button(TextButton::new("audio", "AUDIO", row(8)));
        let back_y = 0.14 + 9.0 * 0.08 + 0.01;
        window.add_button(TextButton::new("back", "BACK",
            (0.35, back_y, 0.65, back_y + 0.065)));

        // ── Audio sub-menu ─────────────────────────────────────────────────────
        let mut audio_window = Window::new()
            .with_overlay(0.6)
            .with_title("AUDIO", (0.35, 0.28, 0.65, 0.38));
        audio_window.add_button(TextButton::new_toggle("music",
            &["MUSIC: OFF", "MUSIC: ON"], (0.30, 0.42, 0.70, 0.52)));
        audio_window.add_slider(Slider::new("volume", "MUSIC VOL", 1.0,
            (0.30, 0.55, 0.70, 0.64)));
        audio_window.add_slider(Slider::new("sfx_volume", "SFX VOL", 1.0,
            (0.30, 0.67, 0.70, 0.76)));
        audio_window.add_button(TextButton::new("back", "BACK",
            (0.35, 0.82, 0.65, 0.90)));

        OptionsMenuRenderer { window, audio_window, audio_open: false }
    }

    pub fn draw(
        &mut self,
        fog_idx: usize, chunk_radius_idx: usize, entity_sim_radius_idx: usize,
        outline_enabled: bool, stats_enabled: bool, hi_res: bool,
        chunk_outlines: bool, entity_outlines: bool,
        music_enabled: bool, music_volume: f32, sfx_volume: f32,
        win_w: f32, win_h: f32,
    ) {
        if self.audio_open {
            self.audio_window.button_mut("music").unwrap().set_label(music_enabled as usize);
            self.audio_window.slider_mut("volume").unwrap().set_value(music_volume);
            self.audio_window.slider_mut("sfx_volume").unwrap().set_value(sfx_volume);
            self.audio_window.draw(win_w, win_h);
        } else {
            self.window.button_mut("fog").unwrap().set_label(fog_idx);
            self.window.button_mut("chunks").unwrap().set_label(chunk_radius_idx);
            self.window.button_mut("sim_radius").unwrap().set_label(entity_sim_radius_idx);
            self.window.button_mut("outline").unwrap().set_label(outline_enabled as usize);
            self.window.button_mut("stats").unwrap().set_label(stats_enabled as usize);
            self.window.button_mut("res").unwrap().set_label(hi_res as usize);
            self.window.button_mut("chunk_outlines").unwrap().set_label(chunk_outlines as usize);
            self.window.button_mut("entity_outlines").unwrap().set_label(entity_outlines as usize);
            self.window.draw(win_w, win_h);
        }
    }

    pub fn handle_click(&mut self, mx: f32, my: f32, win_w: f32, win_h: f32) -> Option<&str> {
        let nx = mx / win_w;
        let ny = my / win_h;
        if self.audio_open {
            // .any() drops its borrow before we mutate audio_open
            if self.audio_window.buttons.iter().any(|b| b.id == "back" && b.is_hit(nx, ny)) {
                self.audio_open = false;
                return None;
            }
            self.audio_window.handle_click(nx, ny)
        } else {
            if self.window.buttons.iter().any(|b| b.id == "audio" && b.is_hit(nx, ny)) {
                self.audio_open = true;
                return None;
            }
            self.window.handle_click(nx, ny)
        }
    }

    /// Returns `Some(("volume", v))` or `Some(("sfx_volume", v))` while dragging
    /// the corresponding slider, `None` otherwise.
    pub fn handle_drag(&self, mx: f32, my: f32, win_w: f32, win_h: f32) -> Option<(&'static str, f32)> {
        if !self.audio_open { return None; }
        self.audio_window.handle_drag(mx / win_w, my / win_h)
            .and_then(|(id, v)| match id {
                "volume"     => Some(("volume", v)),
                "sfx_volume" => Some(("sfx_volume", v)),
                _            => None,
            })
    }
}
