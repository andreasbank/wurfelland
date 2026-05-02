use crate::renderer::ui::{UiRenderer, TextTexture, create_text_texture_scaled};

fn make_tex(text: &str) -> TextTexture { create_text_texture_scaled(text, 2) }

const GAME_NAME: &str = "WURFELLAND";
const GAME_VERSION: &str = "0.1.0";

const PANEL_TOP: f32    = 0.5;
const PANEL_BOTTOM: f32 = 1.0;
const PAD_X: f32        = 0.012;
const INPUT_TOP: f32    = 0.958;
const LINE_H: f32       = 0.032;
const TEXT_H: f32       = 0.025;

pub enum ConsoleAction {
    None,
    Exit,
}

pub struct ConsoleRenderer {
    renderer:     UiRenderer,
    output_lines: Vec<TextTexture>,
    input:        String,
    input_tex:    Option<TextTexture>,
    input_dirty:  bool,
}

impl ConsoleRenderer {
    pub fn new() -> Self {
        ConsoleRenderer {
            renderer:     UiRenderer::new(),
            output_lines: Vec::new(),
            input:        String::new(),
            input_tex:    None,
            input_dirty:  true,
        }
    }

    pub fn type_char(&mut self, c: char) {
        if c.is_ascii() && !c.is_control() {
            self.input.push(c);
            self.input_dirty = true;
        }
    }

    pub fn backspace(&mut self) {
        self.input.pop();
        self.input_dirty = true;
    }

    pub fn submit(&mut self) -> ConsoleAction {
        let trimmed = self.input.trim().to_string();
        let cmd = trimmed.to_lowercase();
        if !trimmed.is_empty() {
            self.push_line(&format!("> {}", trimmed.to_uppercase()));
        }
        self.input.clear();
        self.input_dirty = true;

        match cmd.as_str() {
            "help" => {
                self.push_line(&format!("{} V{}", GAME_NAME, GAME_VERSION));
                self.push_line("COMMANDS:");
                self.push_line("  HELP  -  SHOW THIS MESSAGE");
                self.push_line("  EXIT  -  QUIT THE GAME");
            }
            "exit" => return ConsoleAction::Exit,
            "" => {}
            _ => {
                self.push_line(&format!("UNKNOWN COMMAND: {}", cmd.to_uppercase()));
            }
        }
        ConsoleAction::None
    }

    fn push_line(&mut self, text: &str) {
        let tex = make_tex(text);
        self.output_lines.push(tex);
        // Keep a bounded history — anything older than ~14 lines is scrolled away anyway
        if self.output_lines.len() > 50 {
            self.output_lines.remove(0);
        }
    }

    pub fn draw(&mut self, win_w: f32, win_h: f32) {
        if self.input_dirty {
            let display = format!("> {}", self.input.to_uppercase());
            self.input_tex = Some(make_tex(&display));
            self.input_dirty = false;
        }

        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::CULL_FACE);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        }

        // Background panel
        self.renderer.draw_rect(0.0, PANEL_TOP, 1.0, PANEL_BOTTOM, 0.05, 0.05, 0.08, 0.88);

        // Top border
        self.renderer.draw_rect(0.0, PANEL_TOP, 1.0, PANEL_TOP + 0.003, 0.4, 0.5, 0.8, 1.0);

        // Separator above input
        self.renderer.draw_rect(0.0, INPUT_TOP - 0.003, 1.0, INPUT_TOP, 0.3, 0.3, 0.4, 1.0);

        // Input area tint
        self.renderer.draw_rect(0.0, INPUT_TOP, 1.0, PANEL_BOTTOM, 0.08, 0.08, 0.12, 0.9);

        // Output lines — draw newest at bottom, oldest upward
        let output_area_bottom = INPUT_TOP - 0.006;
        let max_y_top = PANEL_TOP + 0.008;

        for (i, tex) in self.output_lines.iter().rev().enumerate() {
            let y_bottom = output_area_bottom - i as f32 * LINE_H;
            let y_top    = y_bottom - TEXT_H;
            if y_top < max_y_top {
                break;
            }
            let x1 = (PAD_X + tex.pixel_width as f32 / win_w).min(1.0 - PAD_X);
            // Keep the text height proportional to the font's natural aspect ratio
            let natural_h = tex.pixel_height as f32 / win_h;
            let y_mid = (y_top + y_bottom) / 2.0;
            self.renderer.draw_text(tex, PAD_X, y_mid - natural_h / 2.0, x1, y_mid + natural_h / 2.0);
        }

        // Input line
        if let Some(ref tex) = self.input_tex {
            let mid  = (INPUT_TOP + PANEL_BOTTOM) / 2.0;
            let natural_h = tex.pixel_height as f32 / win_h;
            let x1 = (PAD_X + tex.pixel_width as f32 / win_w).min(1.0 - PAD_X);
            self.renderer.draw_text(tex, PAD_X, mid - natural_h / 2.0, x1, mid + natural_h / 2.0);
        }

        unsafe {
            gl::Disable(gl::BLEND);
            gl::Enable(gl::CULL_FACE);
            gl::Enable(gl::DEPTH_TEST);
        }
    }
}
