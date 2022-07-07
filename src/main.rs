#![feature(trait_alias)]

use std::collections::{HashMap, HashSet};

use macroquad::prelude::*;

mod logging;
mod prelude {
    pub const SQUARE_SIZE: f32 = 90.0; // TODO: get from rules
    pub use crate::logging::*;
}

use prelude::*;

type Piece = (usize, usize, String);
type PiecePlacements = HashMap<(usize, usize), String>;

trait SetupRuleFn = Fn() -> Vec<Piece>;
// FIXME: will also need game history for castling and en passant
trait MovementRuleFn = Fn(&Piece, &PiecePlacements) -> HashSet<Piece>;

struct Rules {
    pub piece_name_to_offsets: HashMap<String, (usize, usize)>,
    pub setup_rules: HashMap<String, Box<dyn SetupRuleFn>>,
    pub movement_rules: HashMap<String, Box<dyn MovementRuleFn>>,
}

#[derive(Clone, Copy, Debug)]
struct DraggingState {
    pub source_rc: (usize, usize),
    pub piece_off_x: f32,
    pub piece_off_y: f32,
}

enum InputState {
    NotDragging,
    Dragging(DraggingState),
}

struct Game {
    pieces_sprite: Texture2D,
    piece_placements: PiecePlacements,
    rules: Rules,
    input: InputState,
}

impl Game {
    pub async fn new() -> Self {
        let mut s = Self {
            pieces_sprite: load_texture("assets/pieces.png")
                .await
                .expect("Couldn't load pieces sprint sheet"),
            piece_placements: HashMap::new(),
            rules: Rules {
                piece_name_to_offsets: Rules::default_piece_name_to_offsets(),
                setup_rules: Rules::default_setup_rules(),
                movement_rules: Rules::default_movement_rules(),
            },
            input: InputState::NotDragging,
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

    pub fn handle_input(&mut self) {
        let pos = mouse_position();
        let r = 8 - (pos.1 as usize / SQUARE_SIZE as usize); // TODO: get from rules
        let c = 1 + pos.0 as usize / SQUARE_SIZE as usize;
        match self.input {
            InputState::NotDragging => {
                if is_mouse_button_pressed(MouseButton::Left) {
                    log!("Clicked ({}, {})", r, c);
                    if self.piece_placements.contains_key(&(r, c)) {
                        self.input = InputState::Dragging(DraggingState {
                            source_rc: (r, c),
                            piece_off_x: pos.0 % SQUARE_SIZE,
                            piece_off_y: pos.1 % SQUARE_SIZE,
                        })
                    }
                }
            }
            InputState::Dragging(drag) => {
                if is_mouse_button_released(MouseButton::Left) {
                    log!("Released ({}, {})", r, c);
                    // TODO: we might not need to check bounds, because macroquad doesn't seem to
                    // track the mouse outside of the canvas. Get bounds from rules anyway.
                    if 1 <= r && r <= 8 && 1 <= c && c <= 8 {
                        if let Some(source_piece) = self.piece_placements.get(&drag.source_rc) {
                            if self.is_legal(source_piece, drag.source_rc, (r, c)) {
                                let piece = source_piece.clone();
                                self.piece_placements.remove(&drag.source_rc);
                                self.piece_placements.remove(&(r, c));
                                self.piece_placements.insert((r, c), piece);
                            }
                        }
                    }
                    self.input = InputState::NotDragging;
                }
            }
        }
    }

    fn is_legal(&self, piece_name: &str, from: (usize, usize), to: (usize, usize)) -> bool {
        for (_, r) in self.rules.movement_rules.iter() {
            let piece = (from.0, from.1, piece_name.to_string());  // TODO: avoid to_string 
            let allowed = r(&piece, &self.piece_placements);
            if allowed.contains(&(to.0, to.1, piece_name.to_string())) {
                return true;
            }
        }
        false
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
            let (x, y) = match self.input {
                InputState::Dragging(drag) if drag.source_rc == (*r, *c) => {
                    let pos = mouse_position();
                    (pos.0 - drag.piece_off_x, pos.1 - drag.piece_off_y)
                },
                _ => self.rc_to_xy(*r, *c),
            };
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

    pub fn default_movement_rules() -> HashMap<String, Box<dyn MovementRuleFn>> {
        let mut hm = HashMap::<String, Box<dyn MovementRuleFn>>::new();
        hm.insert("pawn-movement".to_string(), Box::new(|p: &Piece, pp: &PiecePlacements| {
            let mut hs = HashSet::new();
            let dir: i32 = if p.2.chars().nth(0).unwrap().is_uppercase() {
                1
            } else {
                -1
            };
            let max = if (dir == 1 && p.0 == 2) || (dir == -1 && p.0 == 7) {
                2
            } else {
                1
            };
            for i in 1..=max {
                let rc = ((p.0 as i32 + dir * i) as usize, p.1);
                if rc.0 <= 8 && !pp.contains_key(&rc) {
                    hs.insert((rc.0, rc.1, p.2.to_string()));
                }
            }
            hs
        }));
        hm
    }
}

#[macroquad::main("Chess")]
async fn main() {
    let mut game = Game::new().await;
    loop {
        game.draw();
        game.handle_input();
        next_frame().await
    }
}
