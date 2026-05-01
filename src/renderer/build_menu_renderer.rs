use crate::renderer::ui::{Window, TextTexture, create_text_texture};

// ── Layout constants (normalised [0,1] screen space, Y down) ──────────────────
const PANEL_X0: f32 = 0.10;
const PANEL_Y0: f32 = 0.08;
const PANEL_X1: f32 = 0.90;
const PANEL_Y1: f32 = 0.92;

const TITLE_X0: f32 = 0.38;
const TITLE_Y0: f32 = 0.11;
const TITLE_X1: f32 = 0.62;
const TITLE_Y1: f32 = 0.18;

const LIST_X0: f32 = 0.12;
const LIST_Y0: f32 = 0.20;
const LIST_X1: f32 = 0.42;
const LIST_Y1: f32 = 0.90;

const DESC_X0: f32 = 0.44;
const DESC_Y0: f32 = 0.20;
const DESC_X1: f32 = 0.88;
const DESC_Y1: f32 = 0.90;

const ITEM_H:     f32 = 0.065;
const ITEM_GAP:   f32 = 0.006;
const ITEM_PAD_X: f32 = 0.012;
const ITEM_PAD_Y: f32 = 0.010;

const DESC_LINE_H:   f32 = 0.045;
const DESC_LINE_GAP: f32 = 0.008;

struct BuildItem {
    id:     &'static str,
    name:   TextTexture,
    desc:   Vec<TextTexture>,
    bounds: (f32, f32, f32, f32),
}

impl BuildItem {
    fn is_hit(&self, nx: f32, ny: f32) -> bool {
        let (x0, y0, x1, y1) = self.bounds;
        nx >= x0 && nx <= x1 && ny >= y0 && ny <= y1
    }
}

pub struct BuildMenuRenderer {
    window: Window,
    title:  TextTexture,
    items:  Vec<BuildItem>,
}

impl BuildMenuRenderer {
    pub fn new() -> Self {
        let mut items: Vec<BuildItem> = Vec::new();

        let mut add = |id: &'static str, name: &str, desc_lines: &[&str]| {
            let i = items.len();
            let y0 = LIST_Y0 + i as f32 * (ITEM_H + ITEM_GAP);
            items.push(BuildItem {
                id,
                name: create_text_texture(name),
                desc: desc_lines.iter().map(|s| create_text_texture(s)).collect(),
                bounds: (LIST_X0 + 0.004, y0, LIST_X1 - 0.004, y0 + ITEM_H),
            });
        };

        add("bed", "BED", &[
            "A COZY BED.",
            "SLEEP TO SKIP",
            "THE NIGHT.",
        ]);

        BuildMenuRenderer {
            window: Window::new(),
            title:  create_text_texture("BUILD"),
            items,
        }
    }

    /// `mouse_nx`/`mouse_ny` are already normalised to [0,1].
    pub fn draw(&self, mouse_nx: f32, mouse_ny: f32) {
        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::CULL_FACE);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        }

        // Dark overlay
        self.window.draw_rect(0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.60);

        // Panel background
        self.window.draw_rect(PANEL_X0, PANEL_Y0, PANEL_X1, PANEL_Y1, 0.15, 0.15, 0.15, 0.97);

        // Title
        self.window.draw_text(&self.title, TITLE_X0, TITLE_Y0, TITLE_X1, TITLE_Y1);

        // Divider between title and body
        self.window.draw_rect(PANEL_X0 + 0.02, TITLE_Y1 + 0.010,
            PANEL_X1 - 0.02, TITLE_Y1 + 0.014, 0.40, 0.40, 0.40, 1.0);

        // List panel background
        self.window.draw_rect(LIST_X0, LIST_Y0, LIST_X1, LIST_Y1, 0.10, 0.10, 0.12, 1.0);

        // Description panel background
        self.window.draw_rect(DESC_X0, DESC_Y0, DESC_X1, DESC_Y1, 0.10, 0.10, 0.12, 1.0);

        // Vertical divider between list and desc
        self.window.draw_rect(LIST_X1 + 0.005, LIST_Y0,
            LIST_X1 + 0.009, LIST_Y1, 0.35, 0.35, 0.35, 1.0);

        // Find hovered item
        let hovered = self.items.iter().find(|item| item.is_hit(mouse_nx, mouse_ny));

        // Draw item list
        for item in &self.items {
            let (x0, y0, x1, y1) = item.bounds;
            let is_hovered = hovered.map_or(false, |h| h.id == item.id);

            if is_hovered {
                self.window.draw_rect(x0, y0, x1, y1, 0.28, 0.30, 0.45, 1.0);
            } else {
                self.window.draw_rect(x0, y0, x1, y1, 0.18, 0.18, 0.22, 1.0);
            }

            self.window.draw_text(
                &item.name,
                x0 + ITEM_PAD_X, y0 + ITEM_PAD_Y,
                x1 - ITEM_PAD_X, y1 - ITEM_PAD_Y,
            );
        }

        // Draw description for hovered item
        if let Some(item) = hovered {
            let mut dy = DESC_Y0 + 0.025;
            for line in &item.desc {
                self.window.draw_text(
                    line,
                    DESC_X0 + 0.018, dy,
                    DESC_X1 - 0.018, dy + DESC_LINE_H,
                );
                dy += DESC_LINE_H + DESC_LINE_GAP;
            }
        }

        unsafe {
            gl::Disable(gl::BLEND);
            gl::Enable(gl::CULL_FACE);
            gl::Enable(gl::DEPTH_TEST);
        }
    }

    /// Returns the id of the clicked build item, or `None`.
    pub fn handle_click(&self, mouse_x: f32, mouse_y: f32, win_w: f32, win_h: f32) -> Option<&str> {
        let nx = mouse_x / win_w;
        let ny = mouse_y / win_h;
        self.items.iter()
            .find(|item| item.is_hit(nx, ny))
            .map(|item| item.id)
    }
}
