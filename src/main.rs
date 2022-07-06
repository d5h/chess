#![feature(trait_alias)]

use std::collections::HashMap;

use macroquad::prelude::*;

mod logging;
mod prelude {
    pub const SQUARE_SIZE: f32 = 90.0; // TODO: get from rules
    pub use crate::logging::*;
}

use prelude::*;

trait SetupRuleFn = Fn() -> Vec<(usize, usize, String)>;

struct Rules {
    pub setup_rules: HashMap<String, Box<dyn SetupRuleFn>>,
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
        let light = Color::new(0.93, 1.0, 0.98, 1.0);
        let dark = Color::new(0.4, 0.7, 0.7, 1.0);
        clear_background(light);
        for r in 0..8 {
            // TODO: get board size from rules
            for c in 0..8 {
                if (r + c) % 2 == 1 {
                    let y = r as f32 * SQUARE_SIZE;
                    let x = c as f32 * SQUARE_SIZE;
                    draw_rectangle(x, y, SQUARE_SIZE, SQUARE_SIZE, dark);
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
        let y = (8 - r) as f32 * SQUARE_SIZE; // TODO: get board size from rules
        let x = (c - 1) as f32 * SQUARE_SIZE;
        (x, y)
    }
}

impl Rules {
    pub fn default_setup_rules() -> HashMap<String, Box<dyn SetupRuleFn>> {
        let mut hm = HashMap::<String, Box<dyn SetupRuleFn>>::new();
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
        hm.insert(
            "rooks".to_string(),
            Box::new(|| {
                vec![
                    (1, 1, "R".to_string()),
                    (1, 8, "R".to_string()),
                    (8, 1, "r".to_string()),
                    (8, 8, "r".to_string()),
                ]
            }),
        );
        hm.insert(
            "knights".to_string(),
            Box::new(|| {
                vec![
                    (1, 2, "N".to_string()),
                    (1, 7, "N".to_string()),
                    (8, 2, "n".to_string()),
                    (8, 7, "n".to_string()),
                ]
            }),
        );
        hm.insert(
            "bishops".to_string(),
            Box::new(|| {
                vec![
                    (1, 3, "B".to_string()),
                    (1, 6, "B".to_string()),
                    (8, 3, "b".to_string()),
                    (8, 6, "b".to_string()),
                ]
            }),
        );
        hm.insert(
            "queens".to_string(),
            Box::new(|| vec![(1, 4, "Q".to_string()), (8, 4, "q".to_string())]),
        );
        hm.insert(
            "kings".to_string(),
            Box::new(|| vec![(1, 5, "K".to_string()), (8, 5, "k".to_string())]),
        );
        hm
    }

    pub fn default_piece_name_to_offsets() -> HashMap<String, (usize, usize)> {
        let mut hm = HashMap::new();
        let pieces = ["k", "q", "b", "n", "r", "p"];
        for (i, p) in pieces.iter().enumerate() {
            hm.insert(p.to_uppercase(), (i * SQUARE_SIZE as usize, 0));
            hm.insert(
                p.to_lowercase(),
                (i * SQUARE_SIZE as usize, SQUARE_SIZE as usize),
            );
        }
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
