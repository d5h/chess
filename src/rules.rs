use std::collections::{HashSet, HashMap};

use crate::prelude::*;

// We need to marshal Piece data from Rust to JS efficiently. We'll use a representation that can
// be easily and efficiently accessed from JS. This allows JS to directly read and write WASM
// memory, and avoid having to copy data more than necessary.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
#[repr(C, packed)]
pub struct Piece {
    pub row: u8,
    pub col: u8,
    pub name: u8,  // ASCII character
}
// We want a data structure that allows us to quickly lookup what piece is on which square.
// Here again though, we need to marshal this data to and from JS. Hence, we can't use anything
// fancy like a HashMap. We'll represent the board as a 2x2 array of u8, where the value is the
// piece name (ASCII char), or 0 if the square is empty. We add 1 to each dimension because we
// index it starting with 1, in accordance with traditional chess notation.
pub type PiecePlacements = [[u8; 8 + 1]; 8 + 1];  // TODO: don't hardcode board dimensions


pub trait SetupRuleFn = Fn() -> Vec<Piece>;
// FIXME: will also need game history for castling and en passant
// FIXME: need to be able to remove a piece on a different square than where the piece moves
//        for en passant
pub trait MovementRuleFn = Fn(Piece, &PiecePlacements) -> HashSet<Piece>;

extern "C" {
    // JS plugins
    fn movement_plugin(piece_ptr: u32, placements_ptr: u32, retval_ptr: u32, retval_len: u32);
}

pub struct Rules<'a> {
    // Key: piece ASCII code. Value: coordinates in sprite sheet.
    pub piece_name_to_offsets: HashMap<u8, (usize, usize)>,
    // Key: rule name. Value: a callable that returns some piece locations.
    pub setup_rules: HashMap<&'a str, Box<dyn SetupRuleFn>>,
    // Key: rule name. Value: a callable that returns allowed moves for a given piece.
    pub movement_rules: HashMap<&'a str, Box<dyn MovementRuleFn>>,
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
        if !cfg!(test) {
            hm.insert("js-plugin", Box::new(|p: Piece, pp: &PiecePlacements| {
                plugin_movement_rule(p, pp)
            }));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_pawn_moves() {
        let board = "
            rnbqkbnr
            pppppppp
            ........
            ........
            ........
            ........
            PPPPPPPP
            RNBQKBNR
        ";
        // Test white pieces, each column
        for col in 1..=8 {
            let piece = Piece { row: 2, col, name: 'P' as u8 };
            let allowed = vec![
                Piece { row: 3, col, name: 'P' as u8 },
                Piece { row: 4, col, name: 'P' as u8 }
            ];
            assert_moves_allowed_eq(board, piece, allowed);
        }
        // Test black pieces, each column
        for col in 1..=8 {
            let piece = Piece { row: 7, col, name: 'p' as u8 };
            let allowed = vec![
                Piece { row: 6, col, name: 'p' as u8 },
                Piece { row: 5, col, name: 'p' as u8 }
            ];
            assert_moves_allowed_eq(board, piece, allowed);
        }
    }

    #[test]
    fn test_2nd_pawn_moves() {
        let board = "
            rnbqkbnr
            ppppppp.
            .......p
            ........
            ........
            P.......
            .PPPPPPP
            RNBQKBNR
        ";
        // White
        let piece = Piece { row: 3, col: 1, name: 'P' as u8 };
        let allowed = vec![
            Piece { row: 4, col: 1, name: 'P' as u8 }
        ];
        assert_moves_allowed_eq(board, piece, allowed);
        // Black
        let piece = Piece { row: 6, col: 8, name: 'p' as u8 };
        let allowed = vec![
            Piece { row: 5, col: 8, name: 'p' as u8 }
        ];
        assert_moves_allowed_eq(board, piece, allowed);
    }

    #[test]
    fn test_blocked_pawn_moves() {
        let board = "
            rnbqkbnr
            .ppppppp
            ........
            p.......
            P.......
            ........
            .PPPPPPP
            RNBQKBNR
        ";
        // White
        let piece = Piece { row: 4, col: 1, name: 'P' as u8 };
        assert_moves_allowed_eq(board, piece, Vec::new());
        // Black
        let piece = Piece { row: 5, col: 1, name: 'p' as u8 };
        assert_moves_allowed_eq(board, piece, Vec::new());
    }

    fn assert_moves_allowed_eq(board: &str, piece: Piece, expect_allowed: Vec<Piece>) {
        let mut expect_allowed: HashSet<Piece> = expect_allowed.into_iter().collect();
        let rules = Rules::default_movement_rules();
        let placements = string_board_to_placements(board);
        for (_, r) in rules.iter() {
            let allowed = r(piece, &placements);
            for p in allowed.iter() {
                assert!(expect_allowed.contains(p));
                // Test that we can't have multiple rules allow the same moves
                expect_allowed.remove(p);
            }
        }
        assert!(expect_allowed.is_empty());
    }

    fn string_board_to_placements(board: &str) -> PiecePlacements {
        let board = board.trim();
        let mut placements = [[0; 8 + 1]; 8 + 1];
        for (i, line) in board.split('\n').enumerate() {
            let r = i + 1;
            for (j, p) in line.trim().chars().enumerate() {
                let c = j + 1;
                if p != '.' {
                    placements[r][c] = p as u8;
                }
            }
        }
        placements
    }
}
