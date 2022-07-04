use macroquad::prelude::*;

mod prelude {
    pub const SQUARE_SIZE: f32 = 90.0;  // TODO: get from rules
}

use prelude::*;

#[macroquad::main("Chess")]
async fn main() {
    loop {
        draw_board();
        next_frame().await
    }
}

fn draw_board() {
    clear_background(BEIGE);
    for r in 0..8 {  // TODO: get board size from rules
        for c in 0..8 {
            if (r + c) % 2 == 1 {
                let y = r as f32 * SQUARE_SIZE;
                let x = c as f32 * SQUARE_SIZE;
                draw_rectangle(x, y, SQUARE_SIZE, SQUARE_SIZE, BROWN);
            }
        }
    }
}
