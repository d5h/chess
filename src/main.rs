#![feature(type_alias_impl_trait)]

use std::collections::HashMap;

use macroquad::prelude::*;

mod logging;
mod prelude {
    pub const SQUARE_SIZE: f32 = 90.0; // TODO: get from rules
    pub use crate::logging::*;
}

use prelude::*;

type SetupRuleFn = impl Fn() -> Vec<(usize, usize, String)>;

struct Rules {
    pub setup_rules: HashMap<String, Box<SetupRuleFn>>,
    pub piece_name_to_offsets: HashMap<String, (usize, usize)>,
}

struct Game {
    pieces_sprite: Texture2D,
    piece_placements: HashMap<(usize, usize), String>,
    rules: Rules,
}

impl Game {
    pub async fn new() -> Self {
        let mut s = Self {
            pieces_sprite: load_texture("assets/pieces.png")
                .await
                .expect("Couldn't load pieces sprint sheet"),
            piece_placements: HashMap::new(),
            rules: Rules {
                setup_rules: Rules::default_setup_rules(),
                piece_name_to_offsets: Rules::default_piece_name_to_offsets(),
            },
        };
        s.setup();
        s
    }

    fn setup(&mut self) {
        for (_, r) in self.rules.setup_rules.iter() {
            let pieces = r();
            for (r, c, n) in pieces.iter() {
                self.piece_placements.insert((*r, *c), n.to_string());
            }
        }
    }

    pub fn draw(&self) {
        self.draw_board();
        self.draw_pieces();
    }

    fn draw_board(&self) {
        clear_background(BEIGE);
        for r in 0..8 {
            // TODO: get board size from rules
            for c in 0..8 {
                if (r + c) % 2 == 1 {
                    let y = r as f32 * SQUARE_SIZE;
                    let x = c as f32 * SQUARE_SIZE;
                    draw_rectangle(x, y, SQUARE_SIZE, SQUARE_SIZE, BROWN);
                }
            }
        }
    }

    fn draw_pieces(&self) {
        for ((r, c), n) in self.piece_placements.iter() {
            let (x, y) = self.rc_to_xy(*r, *c);
            if let Some((sx, sy)) = self.rules.piece_name_to_offsets.get(n) {
                draw_texture_ex(
                    self.pieces_sprite,
                    x,
                    y,
                    WHITE,
                    DrawTextureParams {
                        source: Some(Rect::new(*sx as f32, *sy as f32, SQUARE_SIZE, SQUARE_SIZE)),
                        ..Default::default()
                    },
                );
            }
        }
    }

    fn rc_to_xy(&self, r: usize, c: usize) -> (f32, f32) {
        let y = (8 - r) as f32 * SQUARE_SIZE;  // TODO: get board size from rules
        let x = (c - 1) as f32 * SQUARE_SIZE;
        (x, y)
    }
}

impl Rules {
    pub fn default_setup_rules() -> HashMap<String, Box<SetupRuleFn>> {
        let mut hm = HashMap::new();
        hm.insert(
            "pawns".to_string(),
            Box::new(|| {
                let mut p = Vec::new();
                for c in 1..=8 {
                    // TODO: get from rules
                    p.push((2 as usize, c as usize, "P".to_string()));
                    p.push((7 as usize, c as usize, "p".to_string()));
                }
                p
            }),
        );
        hm
    }

    pub fn default_piece_name_to_offsets() -> HashMap<String, (usize, usize)> {
        let mut hm = HashMap::new();
        hm.insert("P".to_string(), (5 * SQUARE_SIZE as usize, 0));
        hm.insert(
            "p".to_string(),
            (5 * SQUARE_SIZE as usize, SQUARE_SIZE as usize),
        );
        hm
    }
}

#[macroquad::main("Chess")]
async fn main() {
    let game = Game::new().await;
    loop {
        game.draw();
        next_frame().await
    }
}
