use crate::renderer::ui::{UiRenderer, TextButton, create_text_texture_scaled, TextTexture};

const CONTENT_TOP: f32    = 0.24;
const CONTENT_BOTTOM: f32 = 0.82;
const LINE_H: f32         = 0.08;

pub struct CreditsMenuRenderer {
    renderer:   UiRenderer,
    title:      TextTexture,
    lines:      Vec<Option<TextTexture>>,
    back_btn:   TextButton,
    pub scroll_y: f32,
    max_scroll: f32,
}

impl CreditsMenuRenderer {
    pub fn new() -> Self {
        let renderer = UiRenderer::new();
        let title    = create_text_texture_scaled("CREDITS", 3);
        let back_btn = TextButton::new("back", "BACK", (0.38, 0.84, 0.62, 0.92));

        let text = std::fs::read_to_string("assets/credits.txt")
            .unwrap_or_else(|_| String::new());

        let lines: Vec<Option<TextTexture>> = text
            .lines()
            .map(|line| {
                let cleaned: String = line.chars().filter(|&c| c != '"').collect();
                let trimmed = cleaned.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(create_text_texture_scaled(&trimmed, 2))
                }
            })
            .collect();

        let content_height = CONTENT_BOTTOM - CONTENT_TOP;
        let total_height   = lines.len() as f32 * LINE_H;
        let max_scroll     = (total_height - content_height).max(0.0);

        CreditsMenuRenderer { renderer, title, lines, back_btn, scroll_y: 0.0, max_scroll }
    }

    pub fn handle_scroll(&mut self, dy: f32) {
        self.scroll_y = (self.scroll_y - dy * LINE_H).clamp(0.0, self.max_scroll);
    }

    pub fn draw(&self, win_w: f32, win_h: f32, fb_w: i32, fb_h: i32) {
        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        }

        self.renderer.draw_rect(0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.65);
        self.renderer.draw_rect(0.25, 0.08, 0.75, 0.96, 0.10, 0.10, 0.14, 0.90);

        let tw = self.title.pixel_width  as f32 / win_w;
        let th = self.title.pixel_height as f32 / win_h;
        self.renderer.draw_text(&self.title,
            0.5 - tw * 0.5, 0.16 - th * 0.5,
            0.5 + tw * 0.5, 0.16 + th * 0.5);

        // Scissor to content area so scrolling lines don't overdraw title or BACK button
        unsafe {
            gl::Enable(gl::SCISSOR_TEST);
            let sx = (0.25 * fb_w as f32) as i32;
            let sy = ((1.0 - CONTENT_BOTTOM) * fb_h as f32) as i32;
            let sw = (0.50 * fb_w as f32) as i32;
            let sh = ((CONTENT_BOTTOM - CONTENT_TOP) * fb_h as f32) as i32;
            gl::Scissor(sx, sy, sw, sh);
        }

        let mut cy = CONTENT_TOP + LINE_H * 0.5 - self.scroll_y;
        for line_opt in &self.lines {
            let visible = cy + LINE_H * 0.5 > CONTENT_TOP && cy - LINE_H * 0.5 < CONTENT_BOTTOM;
            if visible {
                if let Some(tex) = line_opt {
                    let lw = tex.pixel_width  as f32 / win_w;
                    let lh = tex.pixel_height as f32 / win_h;
                    self.renderer.draw_text(tex,
                        0.5 - lw * 0.5, cy - lh * 0.5,
                        0.5 + lw * 0.5, cy + lh * 0.5);
                }
            }
            cy += LINE_H;
        }

        unsafe { gl::Disable(gl::SCISSOR_TEST); }

        // Scrollbar — only shown when content overflows
        if self.max_scroll > 0.0 {
            let track_h = CONTENT_BOTTOM - CONTENT_TOP;
            self.renderer.draw_rect(0.722, CONTENT_TOP, 0.742, CONTENT_BOTTOM,
                0.15, 0.15, 0.20, 0.80);
            let thumb_h = (track_h * track_h / (track_h + self.max_scroll)).max(0.04);
            let thumb_t = self.scroll_y / self.max_scroll;
            let thumb_y = CONTENT_TOP + thumb_t * (track_h - thumb_h);
            self.renderer.draw_rect(0.722, thumb_y, 0.742, thumb_y + thumb_h,
                0.50, 0.50, 0.62, 0.90);
        }

        self.back_btn.draw(&self.renderer, win_w, win_h);

        unsafe {
            gl::Enable(gl::DEPTH_TEST);
            gl::Disable(gl::BLEND);
        }
    }

    pub fn handle_click(&self, mx: f32, my: f32, win_w: f32, win_h: f32) -> Option<&str> {
        let nx = mx / win_w;
        let ny = my / win_h;
        if self.back_btn.is_hit(nx, ny) { Some("back") } else { None }
    }
}
