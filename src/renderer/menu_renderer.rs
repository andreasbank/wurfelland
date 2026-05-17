use crate::renderer::ui::{Window, TextButton};

pub struct MenuRenderer {
    window: Window,
}

impl MenuRenderer {
    pub fn new() -> Self {
        let mut window = Window::new()
            .with_overlay(0.6)
            .with_title("PAUSED", (0.30, 0.20, 0.70, 0.30));

        window.add_button(TextButton::new(
            "back",
            "BACK",
            (0.30, 0.34, 0.70, 0.42),
        ));
        window.add_button(TextButton::new(
            "save",
            "SAVE",
            (0.30, 0.44, 0.70, 0.52),
        ));
        window.add_button(TextButton::new(
            "options",
            "OPTIONS",
            (0.30, 0.54, 0.70, 0.62),
        ));
        window.add_button(TextButton::new(
            "main_menu",
            "MAIN MENU",
            (0.30, 0.64, 0.70, 0.72),
        ));
        window.add_button(TextButton::new(
            "exit",
            "EXIT",
            (0.30, 0.74, 0.70, 0.82),
        ));

        MenuRenderer { window }
    }

    pub fn draw(&mut self, win_w: f32, win_h: f32) {
        self.window.draw(win_w, win_h);
    }

    /// Returns the id of the clicked button ("save", "options", "main_menu", "exit"), or None.
    /// `mouse_x/y` are raw GLFW pixel coordinates.
    pub fn handle_click(&self, mouse_x: f32, mouse_y: f32, win_w: f32, win_h: f32) -> Option<&str> {
        self.window.handle_click(mouse_x / win_w, mouse_y / win_h)
    }
}
