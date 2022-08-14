#![feature(trait_alias)]

use std::{collections::HashMap, panic, sync::Mutex};

use macroquad::prelude::*;

mod logging;
mod mem;
mod rules;
mod prelude {
    pub const SQUARE_SIZE: f32 = 90.0; // TODO: get from rules
    pub use crate::logging::*;
    pub use crate::mem::*;
    pub use crate::rules::*;
}

use prelude::*;

extern "C" {
    // JS callbacks
    fn on_move(piece_ptr: u32, placements_ptr: u32, retval_ptr: u32, retval_len: u32);
    fn get_player_color() -> usize;
}

#[derive(Clone, Copy, Debug)]
struct JsMove {
    pub src_row: usize,
    pub src_col: usize,
    pub dst_row: usize,
    pub dst_col: usize,
}
// We shouldn't really need a mutex since JS is single-threaded, but it provides
// a warm fuzzy feeling.
static JS_MOVE: Mutex<Option<JsMove>> = Mutex::new(None);

// So JS can tell WASM to make a move
#[no_mangle]
pub extern "C" fn make_move_from_js(
    src_row: usize,
    src_col: usize,
    dst_row: usize,
    dst_col: usize,
) {
    log!("Got a move from JS!");
    let mut m = JS_MOVE.lock().unwrap();
    *m = Some(JsMove {
        src_row,
        src_col,
        dst_row,
        dst_col,
    })
}

static FLIPPED: Mutex<bool> = Mutex::new(false);

#[no_mangle]
pub extern "C" fn flip_board(flipped: u32) {
    let mut f = FLIPPED.lock().unwrap();
    *f = flipped != 0;
}

static RULES_UPDATE: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);

#[no_mangle]
pub extern "C" fn rules_update(json_str_ptr: *const u8) {
    let len = memlen(json_str_ptr);
    let s = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(json_str_ptr, len)) };
    if let Ok(v) = serde_json::from_str::<HashMap<String, bool>>(s) {
        let mut r = RULES_UPDATE.lock().unwrap();
        *r = Some(v);
    }
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
    game_data: GameData,
    input: InputState,
    flipped: bool,
    player: usize, // 0 for white, 1 for black
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
            flipped: false,
            player: 0,
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

    pub fn handle_js_changes(&mut self) {
        {
            let f = FLIPPED.lock().unwrap();
            self.flipped = *f;
            self.player = unsafe { get_player_color() };
        }

        {
            let mut r = RULES_UPDATE.lock().unwrap();
            if let Some(r) = &*r {
                for (&n, m) in self.rules.movement_rules.iter_mut() {
                    if let Some(&a) = r.get(n) {
                        if m.active != a {
                            log!("Toggling {} to {}", n, a);
                            m.active = a;
                        }
                    }
                }
            }
            *r = None;
        }
    }

    pub fn draw(&self) {
        self.draw_board();
        self.draw_pieces();
    }

    pub fn handle_input(&mut self) {
        let pos = mouse_position();
        let (r, c) = self.xy_to_rc(pos.0, pos.1);
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
                    let (sr, sc) = drag.source_rc;
                    self.try_move(self.player, sr, sc, r, c);
                    self.input = InputState::NotDragging;
                }
            }
        }
    }

    pub fn handle_js_move(&mut self) {
        let mut m = JS_MOVE.lock().unwrap();
        if let Some(m) = *m {
            log!("Got a JsMove! {:?}", m);
            self.try_move(1 - self.player, m.src_row, m.src_col, m.dst_row, m.dst_col);
        }
        *m = None;
    }

    fn try_move(&mut self, player: usize, sr: usize, sc: usize, dr: usize, dc: usize) {
        if 1 <= dr && dr <= 8 && 1 <= dc && dc <= 8 {
            let name = self.piece_placements[sr][sc];
            if name != 0 {
                let source_piece = Piece {
                    row: sr as u8,
                    col: sc as u8,
                    name,
                };
                if let Some(m) = self.get_legal(player, source_piece, (dr, dc)) {
                    Rules::make_move(source_piece, m, &mut self.piece_placements);
                    self.game_data = m.game_data;
                    self.game_data.ply += 1;
                    unsafe {
                        on_move(sr as u32, sc as u32, m.dst.row as u32, m.dst.col as u32);
                    }
                }
            }
        }
        self.input = InputState::NotDragging;
    }

    fn get_legal(&self, player: usize, piece: Piece, to: (usize, usize)) -> Option<Move> {
        if !self.is_turn(player, piece) {
            return None;
        }
        self.rules
            .allowed_moves(piece, &self.piece_placements, self.game_data)
            .into_iter()
            .find(|m| m.dst.row == to.0 as u8 && m.dst.col == to.1 as u8)
    }

    fn is_turn(&self, player: usize, piece: Piece) -> bool {
        for (_, r) in self.rules.turn_rules.iter() {
            if r(player, piece, self.game_data) {
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
        // TODO: get board size from rules
        let y = if self.flipped { r - 1 } else { 8 - r } as f32 * SQUARE_SIZE;
        let x = if self.flipped { 8 - c } else { c - 1 } as f32 * SQUARE_SIZE;
        (x, y)
    }

    fn xy_to_rc(&self, x: f32, y: f32) -> (usize, usize) {
        let x = x as usize / SQUARE_SIZE as usize;
        let y = y as usize / SQUARE_SIZE as usize;
        // TODO: get board size from rules
        let r = if self.flipped { y + 1 } else { 8 - y };
        let c = if self.flipped { 8 - x } else { 1 + x };
        (r, c)
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
        game.handle_js_changes();
        game.draw();
        game.handle_input();
        game.handle_js_move();
        next_frame().await
    }
}
