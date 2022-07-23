#![feature(trait_alias)]
#![feature(let_chains)]

use std::panic;

use macroquad::prelude::*;

mod logging;
mod rules;
mod prelude {
    pub const SQUARE_SIZE: f32 = 90.0; // TODO: get from rules
    pub use crate::logging::*;
    pub use crate::rules::*;
}

use prelude::*;

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
    game_data: GameData,
    input: InputState,
}

impl<'a> Game<'a> {
    pub async fn new() -> Game<'a> {
        let mut s = Self {
            pieces_sprite: load_texture("assets/img/pieces.png")
                .await
                .expect("Couldn't load pieces sprite sheet"),
            piece_placements: [[0; 8 + 1]; 8 + 1],
            rules: Rules::defaults(),
            game_data: GameData { ply: 1, mask: 0 },
            input: InputState::NotDragging,
        };
        s.setup();
        s
    }

    fn setup(&mut self) {
        for (_, r) in self.rules.setup_rules.iter() {
            let pieces = r();
            for Piece {
                row: r,
                col: c,
                name: n,
            } in pieces.iter()
            {
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
                            let source_piece = Piece {
                                row: sr as u8,
                                col: sc as u8,
                                name,
                            };
                            if let Some(m) = self.get_legal(source_piece, (r, c)) {
                                self.piece_placements[sr][sc] = 0;
                                self.piece_placements[r][c] = name;
                                match m.typ {
                                    MoveType::Capture { row: cr, col: cc } => {
                                        if (cr as usize, cc as usize) != (r, c) {
                                            self.piece_placements[cr as usize][cc as usize] = 0;
                                        }
                                    }
                                    MoveType::Secondary { src: ss, dst: sd } => {
                                        if (ss.row as usize, ss.col as usize) != (r, c) {
                                            self.piece_placements[ss.row as usize]
                                                [ss.col as usize] = 0;
                                        }
                                        self.piece_placements[sd.row as usize][sd.col as usize] =
                                            sd.name;
                                    }
                                    MoveType::Normal => {}
                                }
                                self.game_data = m.game_data;
                                self.game_data.ply += 1;
                            }
                        }
                    }
                    self.input = InputState::NotDragging;
                }
            }
        }
    }

    fn get_legal(&self, piece: Piece, to: (usize, usize)) -> Option<Move> {
        if !self.is_turn(piece) {
            return None;
        }
        self.rules
            .allowed_moves(piece, &self.piece_placements, self.game_data)
            .into_iter()
            .find(|m| {
                m.dst
                    == Piece {
                        row: to.0 as u8,
                        col: to.1 as u8,
                        name: piece.name,
                    }
            })
    }

    fn is_turn(&self, piece: Piece) -> bool {
        for (_, r) in self.rules.turn_rules.iter() {
            if r(piece, self.game_data) {
                return true;
            }
        }
        return false;
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
        for r in 1..=8 {
            // TODO: don't hard code board dimensions
            for c in 1..=8 {
                let n = self.piece_placements[r][c];
                if n != 0 {
                    let (x, y) = match self.input {
                        InputState::Dragging(drag) if drag.source_rc == (r, c) => {
                            let pos = mouse_position();
                            (pos.0 - drag.piece_off_x, pos.1 - drag.piece_off_y)
                        }
                        _ => self.rc_to_xy(r, c),
                    };
                    if let Some((sx, sy)) = self.rules.piece_name_to_offsets.get(&n) {
                        draw_texture_ex(
                            self.pieces_sprite,
                            x,
                            y,
                            WHITE,
                            DrawTextureParams {
                                source: Some(Rect::new(
                                    *sx as f32,
                                    *sy as f32,
                                    SQUARE_SIZE,
                                    SQUARE_SIZE,
                                )),
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
