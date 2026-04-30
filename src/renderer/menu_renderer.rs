use crate::renderer::ui::{Window, TextButton};

pub struct MenuRenderer {
    window: Window,
}

impl MenuRenderer {
    pub fn new() -> Self {
        let mut window = Window::new()
            .with_overlay(0.6)
            .with_title("PAUSED", (0.30, 0.28, 0.70, 0.38));

        window.add_button(TextButton::new_toggle(
            "outline",
            &["OUTLINE:OFF", "OUTLINE:ON"],
            (0.30, 0.44, 0.70, 0.52),
        ));
        window.add_button(TextButton::new_toggle(
            "res",
            &["RES:LO", "RES:HI"],
            (0.30, 0.56, 0.70, 0.64),
        ));
        window.add_button(TextButton::new(
            "exit",
            "EXIT",
            (0.38, 0.68, 0.62, 0.76),
        ));

        MenuRenderer { window }
    }

    /// Sync toggle-button labels to current game state, then draw.
    pub fn draw(&mut self, outline_enabled: bool, hi_res: bool) {
        self.window.button_mut("outline").unwrap().set_label(outline_enabled as usize);
        self.window.button_mut("res").unwrap().set_label(hi_res as usize);
        self.window.draw();
    }

    /// Returns the id of the clicked button ("exit", "outline", "res"), or None.
    /// `mouse_x/y` are raw GLFW pixel coordinates.
    pub fn handle_click(&self, mouse_x: f32, mouse_y: f32, win_w: f32, win_h: f32) -> Option<&str> {
        self.window.handle_click(mouse_x / win_w, mouse_y / win_h)
    }
}
