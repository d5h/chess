#![feature(trait_alias)]

use std::collections::{HashMap, HashSet};
use std::panic;

use macroquad::prelude::*;

mod logging;
mod prelude {
    pub const SQUARE_SIZE: f32 = 90.0; // TODO: get from rules
    pub use crate::logging::*;
}

use prelude::*;

// We need to marshal Piece data from Rust to JS efficiently. We'll use a representation that can
// be easily and efficiently accessed from JS. This allows JS to directly read and write WASM
// memory, and avoid having to copy data more than necessary.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
#[repr(C, packed)]
struct Piece {
    row: u8,
    col: u8,
    name: u8,  // ASCII character
}
// We want a data structure that allows us to quickly lookup what piece is on which square.
// Here again though, we need to marshal this data to and from JS. Hence, we can't use anything
// fancy like a HashMap. We'll represent the board as a 2x2 array of u8, where the value is the
// piece name (ASCII char), or 0 if the square is empty. We add 1 to each dimension because we
// index it starting with 1, in accordance with traditional chess notation.
type PiecePlacements = [[u8; 8 + 1]; 8 + 1];  // TODO: don't hardcode board dimensions

trait SetupRuleFn = Fn() -> Vec<Piece>;
// FIXME: will also need game history for castling and en passant
// FIXME: need to be able to remove a piece on a different square than where the piece moves
//        for en passant
trait MovementRuleFn = Fn(Piece, &PiecePlacements) -> HashSet<Piece>;

extern "C" {
    // JS plugins
    fn movement_plugin(piece_ptr: u32, placements_ptr: u32, retval_ptr: u32, retval_len: u32);
}

struct Rules<'a> {
    // Key: piece ASCII code. Value: coordinates in sprite sheet.
    pub piece_name_to_offsets: HashMap<u8, (usize, usize)>,
    // Key: rule name. Value: a callable that returns some piece locations.
    pub setup_rules: HashMap<&'a str, Box<dyn SetupRuleFn>>,
    // Key: rule name. Value: a callable that returns allowed moves for a given piece.
    pub movement_rules: HashMap<&'a str, Box<dyn MovementRuleFn>>,
}

// Mouse stuff
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

struct Game<'a> {
    pieces_sprite: Texture2D,
    piece_placements: PiecePlacements,
    rules: Rules<'a>,
    input: InputState,
}

impl<'a> Game<'a> {
    pub async fn new() -> Game<'a> {
        let mut s = Self {
            pieces_sprite: load_texture("assets/img/pieces.png")
                .await
                .expect("Couldn't load pieces sprite sheet"),
            piece_placements: [[0; 8 + 1]; 8 + 1],
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
            for Piece {row: r, col: c, name: n } in pieces.iter() {
                self.piece_placements[*r as usize][*c as usize] = *n;
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
                    if self.piece_placements[r][c] != 0 {
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
                        let (sr, sc) = drag.source_rc;
                        let name = self.piece_placements[sr][sc];
                        if name != 0 {
                            let source_piece = Piece{ row: sr as u8, col: sc as u8, name };
                            if self.is_legal(source_piece, (r, c)) {
                                self.piece_placements[sr][sc] = 0;
                                self.piece_placements[r][c] = name;
                            }
                        }
                    }
                    self.input = InputState::NotDragging;
                }
            }
        }
    }

    fn is_legal(&self, piece: Piece, to: (usize, usize)) -> bool {
        for (_, r) in self.rules.movement_rules.iter() {
            let allowed = r(piece, &self.piece_placements);
            if allowed.contains(&Piece { row: to.0 as u8, col: to.1 as u8, name: piece.name}) {
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
        for r in 1..=8 {  // TODO: don't hard code board dimensions
            for c in 1..=8 {
                let n = self.piece_placements[r][c];
                if n != 0 {
                    let (x, y) = match self.input {
                        InputState::Dragging(drag) if drag.source_rc == (r, c) => {
                            let pos = mouse_position();
                            (pos.0 - drag.piece_off_x, pos.1 - drag.piece_off_y)
                        },
                        _ => self.rc_to_xy(r, c),
                    };
                    if let Some((sx, sy)) = self.rules.piece_name_to_offsets.get(&n) {
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
        }
    }

    fn rc_to_xy(&self, r: usize, c: usize) -> (f32, f32) {
        let y = (8 - r) as f32 * SQUARE_SIZE; // TODO: get board size from rules
        let x = (c - 1) as f32 * SQUARE_SIZE;
        (x, y)
    }
}

impl<'a> Rules<'a> {
    pub fn default_piece_name_to_offsets() -> HashMap<u8, (usize, usize)> {
        let mut hm = HashMap::new();
        let pieces = ['k', 'q', 'b', 'n', 'r', 'p'];
        for (i, p) in pieces.iter().enumerate() {
            hm.insert(p.to_uppercase().nth(0).unwrap() as u8, (i * SQUARE_SIZE as usize, 0));
            hm.insert(
                *p as u8,
                (i * SQUARE_SIZE as usize, SQUARE_SIZE as usize),
            );
        }
        hm
    }

    pub fn default_setup_rules() -> HashMap<&'a str, Box<dyn SetupRuleFn>> {
        let mut hm = HashMap::<&'a str, Box<dyn SetupRuleFn>>::new();
        hm.insert(
            "pawns",
            Box::new(|| {
                let mut p = Vec::new();
                for c in 1..=8 {
                    // TODO: get from rules
                    p.push(Piece { row: 2, col: c, name: 'P' as u8});
                    p.push(Piece { row: 7, col: c, name: 'p' as u8});
                }
                p
            }),
        );
        hm.insert(
            "rooks",
            Box::new(|| {
                vec![
                    Piece { row: 1, col: 1, name: 'R' as u8 },
                    Piece { row: 1, col: 8, name: 'R' as u8 },
                    Piece { row: 8, col: 1, name: 'r' as u8 },
                    Piece { row: 8, col: 8, name: 'r' as u8 },
                ]
            }),
        );
        hm.insert(
            "knights",
            Box::new(|| {
                vec![
                    Piece { row: 1, col: 2, name: 'N' as u8},
                    Piece { row: 1, col: 7, name: 'N' as u8},
                    Piece { row: 8, col: 2, name: 'n' as u8},
                    Piece { row: 8, col: 7, name: 'n' as u8},
                ]
            }),
        );
        hm.insert(
            "bishops",
            Box::new(|| {
                vec![
                    Piece { row: 1, col: 3, name: 'B' as u8 },
                    Piece { row: 1, col: 6, name: 'B' as u8 },
                    Piece { row: 8, col: 3, name: 'b' as u8 },
                    Piece { row: 8, col: 6, name: 'b' as u8 },
                ]
            }),
        );
        hm.insert(
            "queens",
            Box::new(|| vec![Piece { row: 1, col: 4, name: 'Q' as u8}, Piece { row: 8, col: 4, name: 'q' as u8}]),
        );
        hm.insert(
            "kings",
            Box::new(|| vec![Piece { row: 1, col: 5, name: 'K' as u8}, Piece { row: 8, col: 5, name: 'k' as u8}]),
        );
        hm
    }

    pub fn default_movement_rules() -> HashMap<&'a str, Box<dyn MovementRuleFn>> {
        let mut hm = HashMap::<&'a str, Box<dyn MovementRuleFn>>::new();
        hm.insert("pawn-movement", Box::new(|p: Piece, pp: &PiecePlacements| {
            let mut hs = HashSet::new();
            let dir: i32 = if (p.name as char).is_uppercase() {
                1
            } else {
                -1
            };
            let max = if (dir == 1 && p.row == 2) || (dir == -1 && p.row == 7) {
                2
            } else {
                1
            };
            for i in 1..=max {
                let rc = ((p.row as i32 + dir * i) as usize, p.col as usize);
                if rc.0 <= 8 && pp[rc.0][rc.1] == 0 {
                    hs.insert(Piece { row: rc.0 as u8, col: rc.1 as u8, name: p.name});
                }
            }
            hs
        }));
        hm.insert("js-plugin", Box::new(|p: Piece, pp: &PiecePlacements| {
            plugin_movement_rule(p, pp)
        }));
        hm
    }
}

fn plugin_movement_rule(p: Piece, pp: &PiecePlacements) -> HashSet<Piece> {
    let mut hs = HashSet::new();
    let piece_ptr: *const Piece = &p;
    let placements_ptr: *const [u8; 8 + 1] = pp.as_ptr();
    const RETVAL_LEN: usize = 3 * 8 * 8 * 95;
    let mut retval: [u8; RETVAL_LEN] = [0; RETVAL_LEN];
    let retval_ptr: *const u8 = retval.as_mut_ptr();
    unsafe { movement_plugin(piece_ptr as u32, placements_ptr as u32, retval_ptr as u32, RETVAL_LEN as u32); }
    let mut i = 0;
    while i < RETVAL_LEN {
        if retval[i] == 0 {
            break;
        }
        let (r, c, n) = (retval[i], retval[i + 1], retval[i + 2]);
        // FIXME: do some checks here
        hs.insert(Piece { row: r, col: c, name: n });
        i += 3;
    }
    hs
}

pub fn hook(info: &panic::PanicInfo) {
    log!("{}", info.to_string());
}

#[macroquad::main("Chess")]
async fn main() {
    panic::set_hook(Box::new(hook));
    let mut game = Game::new().await;
    loop {
        game.draw();
        game.handle_input();
        next_frame().await
    }
}
