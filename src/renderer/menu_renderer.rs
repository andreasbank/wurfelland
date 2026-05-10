use crate::renderer::ui::{Window, TextButton};

pub struct MenuRenderer {
    window: Window,
}

impl MenuRenderer {
    pub fn new() -> Self {
        let mut window = Window::new()
            .with_overlay(0.6)
            .with_title("PAUSED", (0.30, 0.24, 0.70, 0.34));

        window.add_button(TextButton::new(
            "options",
            "OPTIONS",
            (0.30, 0.44, 0.70, 0.54),
        ));
        window.add_button(TextButton::new(
            "exit",
            "EXIT",
            (0.30, 0.58, 0.70, 0.68),
        ));

        MenuRenderer { window }
    }

    pub fn draw(&mut self, win_w: f32, win_h: f32) {
        self.window.draw(win_w, win_h);
    }

    /// Returns the id of the clicked button ("exit", "outline", "res"), or None.
    /// `mouse_x/y` are raw GLFW pixel coordinates.
    pub fn handle_click(&self, mouse_x: f32, mouse_y: f32, win_w: f32, win_h: f32) -> Option<&str> {
        self.window.handle_click(mouse_x / win_w, mouse_y / win_h)
    }
}
