use std::collections::{HashMap, HashSet};

use crate::prelude::*;

// We need to marshal Piece data from Rust to JS efficiently. We'll use a representation that can
// be easily and efficiently accessed from JS. This allows JS to directly read and write WASM
// memory, and avoid having to copy data more than necessary.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
#[repr(C, packed)]
pub struct Piece {
    pub row: u8,
    pub col: u8,
    pub name: u8, // ASCII character
}
// We want a data structure that allows us to quickly lookup what piece is on which square.
// Here again though, we need to marshal this data to and from JS. Hence, we can't use anything
// fancy like a HashMap. We'll represent the board as a 2x2 array of u8, where the value is the
// piece name (ASCII char), or 0 if the square is empty. We add 1 to each dimension because we
// index it starting with 1, in accordance with traditional chess notation.
pub type PiecePlacements = [[u8; 8 + 1]; 8 + 1]; // TODO: don't hardcode board dimensions

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
#[repr(C, packed)]
pub struct GameData {
    pub ply: u16,
    // Bit mask for things like castle rights. See GD_ flags below
    pub mask: u16,
}

const GD_NO_WHITE_KS_CASTLE: u16 = 0x01;
const GD_NO_BLACK_KS_CASTLE: u16 = 0x02;
const GD_NO_WHITE_QS_CASTLE: u16 = 0x04;
const GD_NO_BLACK_QS_CASTLE: u16 = 0x08;

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum MoveType {
    Normal,
    // The coordinates are redundant with the move, in normal chess, except for en passant.
    Capture { row: u8, col: u8 },
    // Secondary is a second piece to move. In normal chess, this is only the rook during castles.
    Secondary { src: Piece, dst: Piece },
}

// Represents a possible move. Note that the starting piece & square are implicitly known by the
// caller so not included in the generated moves.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub struct Move {
    pub dst: Piece,
    pub typ: MoveType,
    pub game_data: GameData,
}

pub trait SetupRuleFn = Fn() -> Vec<Piece>;
pub trait TurnRuleFn = Fn(Piece, GameData) -> bool;
// FIXME: will also need game history for castling and en passant
// FIXME: need to be able to remove a piece on a different square than where the piece moves
//        for en passant
// FIXME: need to have a rule for resolving checks
pub trait MovementRuleFn = Fn(Piece, &PiecePlacements, GameData, &mut HashSet<Move>);

extern "C" {
    // JS plugins
    fn movement_plugin(piece_ptr: u32, placements_ptr: u32, retval_ptr: u32, retval_len: u32);
}

pub struct MovementRule {
    pub f: Box<dyn MovementRuleFn>,
    pub piece_constrait: Option<char>,
}

pub struct Rules<'a> {
    // Key: piece ASCII code. Value: coordinates in sprite sheet.
    pub piece_name_to_offsets: HashMap<u8, (usize, usize)>,
    // Key: rule name. Value: a callable that returns some piece locations.
    pub setup_rules: HashMap<&'a str, Box<dyn SetupRuleFn>>,
    // Key: rule name. Value: a callable that returns true if the given piece can move.
    pub turn_rules: HashMap<&'a str, Box<dyn TurnRuleFn>>,
    // Key: rule name. Value: a callable that returns allowed moves for a given piece.
    pub movement_rules: HashMap<&'a str, MovementRule>,
}

impl Piece {
    pub fn is_white(&self) -> bool {
        is_piece_white(self.name)
    }
}

fn is_piece_white(n: u8) -> bool {
    (n as char).is_ascii_uppercase()
}

impl Move {
    pub fn normal(r: usize, c: usize, name: u8, game_data: GameData) -> Self {
        Self {
            dst: Piece {
                row: r as u8,
                col: c as u8,
                name: name,
            },
            typ: MoveType::Normal,
            game_data,
        }
    }

    pub fn capture(r: usize, c: usize, name: u8, game_data: GameData) -> Self {
        Self {
            dst: Piece {
                row: r as u8,
                col: c as u8,
                name: name,
            },
            typ: MoveType::Capture {
                row: r as u8,
                col: c as u8,
            },
            game_data,
        }
    }
}

type Directions = [(i32, i32); 4];
const AXES: Directions = [(0, 1), (1, 0), (0, -1), (-1, 0)];
const DIAGONALS: Directions = [(-1, -1), (-1, 1), (1, -1), (1, 1)];

fn add_linear_moves(
    p: Piece,
    pp: &PiecePlacements,
    hs: &mut HashSet<Move>,
    dirs: &Directions,
    max: i32,
    game_data: GameData,
) {
    let is_white = p.is_white();
    for (x, y) in dirs {
        for i in 1..=max {
            let nr = p.row as i32 + y * i;
            let nc = p.col as i32 + x * i;
            if !std_in_bounds(nr, nc) {
                break;
            }
            let (nr, nc) = (nr as usize, nc as usize);
            if pp[nr][nc] != 0 {
                if is_piece_white(pp[nr][nc]) != is_white {
                    hs.insert(Move::capture(nr, nc, p.name, game_data));
                }
                break;
            }
            hs.insert(Move::normal(nr, nc, p.name, game_data));
        }
    }
}

impl<'a> Rules<'a> {
    pub fn defaults() -> Self {
        Self {
            piece_name_to_offsets: Self::default_piece_name_to_offsets(),
            setup_rules: Self::default_setup_rules(),
            turn_rules: Self::default_turn_rules(),
            movement_rules: Self::default_movement_rules(),
        }
    }

    pub fn default_piece_name_to_offsets() -> HashMap<u8, (usize, usize)> {
        let mut hm = HashMap::new();
        let pieces = ['k', 'q', 'b', 'n', 'r', 'p'];
        for (i, p) in pieces.iter().enumerate() {
            hm.insert(
                p.to_uppercase().nth(0).unwrap() as u8,
                (i * SQUARE_SIZE as usize, 0),
            );
            hm.insert(*p as u8, (i * SQUARE_SIZE as usize, SQUARE_SIZE as usize));
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
                    p.push(Piece {
                        row: 2,
                        col: c,
                        name: 'P' as u8,
                    });
                    p.push(Piece {
                        row: 7,
                        col: c,
                        name: 'p' as u8,
                    });
                }
                p
            }),
        );
        hm.insert(
            "rooks",
            Box::new(|| {
                vec![
                    Piece {
                        row: 1,
                        col: 1,
                        name: 'R' as u8,
                    },
                    Piece {
                        row: 1,
                        col: 8,
                        name: 'R' as u8,
                    },
                    Piece {
                        row: 8,
                        col: 1,
                        name: 'r' as u8,
                    },
                    Piece {
                        row: 8,
                        col: 8,
                        name: 'r' as u8,
                    },
                ]
            }),
        );
        hm.insert(
            "knights",
            Box::new(|| {
                vec![
                    Piece {
                        row: 1,
                        col: 2,
                        name: 'N' as u8,
                    },
                    Piece {
                        row: 1,
                        col: 7,
                        name: 'N' as u8,
                    },
                    Piece {
                        row: 8,
                        col: 2,
                        name: 'n' as u8,
                    },
                    Piece {
                        row: 8,
                        col: 7,
                        name: 'n' as u8,
                    },
                ]
            }),
        );
        hm.insert(
            "bishops",
            Box::new(|| {
                vec![
                    Piece {
                        row: 1,
                        col: 3,
                        name: 'B' as u8,
                    },
                    Piece {
                        row: 1,
                        col: 6,
                        name: 'B' as u8,
                    },
                    Piece {
                        row: 8,
                        col: 3,
                        name: 'b' as u8,
                    },
                    Piece {
                        row: 8,
                        col: 6,
                        name: 'b' as u8,
                    },
                ]
            }),
        );
        hm.insert(
            "queens",
            Box::new(|| {
                vec![
                    Piece {
                        row: 1,
                        col: 4,
                        name: 'Q' as u8,
                    },
                    Piece {
                        row: 8,
                        col: 4,
                        name: 'q' as u8,
                    },
                ]
            }),
        );
        hm.insert(
            "kings",
            Box::new(|| {
                vec![
                    Piece {
                        row: 1,
                        col: 5,
                        name: 'K' as u8,
                    },
                    Piece {
                        row: 8,
                        col: 5,
                        name: 'k' as u8,
                    },
                ]
            }),
        );
        hm
    }

    pub fn default_turn_rules() -> HashMap<&'a str, Box<dyn TurnRuleFn>> {
        let mut hm = HashMap::<&'a str, Box<dyn TurnRuleFn>>::new();
        hm.insert(
            "player-order",
            Box::new(|p: Piece, gd: GameData| p.is_white() == (gd.ply % 2 == 1)),
        );
        hm
    }

    pub fn default_movement_rules() -> HashMap<&'a str, MovementRule> {
        let mut hm = HashMap::<&'a str, MovementRule>::new();
        hm.insert(
            "pawn-movement",
            MovementRule {
                piece_constrait: Some('p'),
                f: Box::new(
                    |p: Piece, pp: &PiecePlacements, gd: GameData, hs: &mut HashSet<Move>| {
                        let dir: i32 = if p.is_white() { 1 } else { -1 };
                        let max = if (dir == 1 && p.row == 2) || (dir == -1 && p.row == 7) {
                            2
                        } else {
                            1
                        };
                        for i in 1..=max {
                            let rc = ((p.row as i32 + dir * i) as usize, p.col as usize);
                            if rc.0 <= 8 && pp[rc.0][rc.1] == 0 {
                                hs.insert(Move::normal(rc.0, rc.1, p.name, gd));
                            }
                        }
                    },
                ),
            },
        );
        hm.insert(
            "pawn-capture",
            MovementRule {
                piece_constrait: Some('p'),
                f: Box::new(
                    |p: Piece, pp: &PiecePlacements, gd: GameData, hs: &mut HashSet<Move>| {
                        let dir: i8 = if p.is_white() { 1 } else { -1 };
                        for i in [-1, 1] {
                            let r = (p.row as i8 + dir) as usize;
                            let c = (p.col as i8 + i) as usize;
                            if 1 <= c
                                && c <= 8
                                && pp[r][c] != 0
                                && is_piece_white(pp[r][c]) != p.is_white()
                            {
                                hs.insert(Move::capture(r, c, p.name, gd));
                            }
                        }
                    },
                ),
            },
        );
        hm.insert(
            "knight",
            MovementRule {
                piece_constrait: Some('n'),
                f: Box::new(
                    |p: Piece, pp: &PiecePlacements, gd: GameData, hs: &mut HashSet<Move>| {
                        let is_white = p.is_white();
                        for (x, y) in [
                            (1, 2),
                            (2, 1),
                            (2, -1),
                            (1, -2),
                            (-1, -2),
                            (-2, -1),
                            (-2, 1),
                            (-1, 2),
                        ] {
                            let nr = p.row as i32 + y;
                            let nc = p.col as i32 + x;
                            if !std_in_bounds(nr, nc) {
                                continue;
                            }
                            let (nr, nc) = (nr as usize, nc as usize);
                            if pp[nr][nc] != 0 {
                                if is_piece_white(pp[nr][nc]) != is_white {
                                    hs.insert(Move::capture(nr, nc, p.name, gd));
                                }
                            } else {
                                hs.insert(Move::normal(nr, nc, p.name, gd));
                            }
                        }
                    },
                ),
            },
        );
        hm.insert(
            "bishop",
            MovementRule {
                piece_constrait: Some('b'),
                f: Box::new(
                    |p: Piece, pp: &PiecePlacements, gd: GameData, hs: &mut HashSet<Move>| {
                        add_linear_moves(p, pp, hs, &DIAGONALS, 8, gd);
                    },
                ),
            },
        );
        hm.insert(
            "rook",
            MovementRule {
                piece_constrait: Some('r'),
                f: Box::new(
                    |p: Piece, pp: &PiecePlacements, gd: GameData, hs: &mut HashSet<Move>| {
                        let gd = match (p.row, p.col) {
                            (1, 1) => GameData {
                                mask: gd.mask | GD_NO_WHITE_QS_CASTLE,
                                ..gd
                            },
                            (1, 8) => GameData {
                                mask: gd.mask | GD_NO_WHITE_KS_CASTLE,
                                ..gd
                            },
                            (8, 1) => GameData {
                                mask: gd.mask | GD_NO_BLACK_QS_CASTLE,
                                ..gd
                            },
                            (8, 8) => GameData {
                                mask: gd.mask | GD_NO_BLACK_KS_CASTLE,
                                ..gd
                            },
                            _ => gd,
                        };
                        add_linear_moves(p, pp, hs, &AXES, 8, gd);
                    },
                ),
            },
        );
        hm.insert(
            "queen",
            MovementRule {
                piece_constrait: Some('q'),
                f: Box::new(
                    |p: Piece, pp: &PiecePlacements, gd: GameData, hs: &mut HashSet<Move>| {
                        add_linear_moves(p, pp, hs, &AXES, 8, gd);
                        add_linear_moves(p, pp, hs, &DIAGONALS, 8, gd);
                    },
                ),
            },
        );
        hm.insert(
            "king",
            MovementRule {
                piece_constrait: Some('k'),
                f: Box::new(
                    |p: Piece, pp: &PiecePlacements, gd: GameData, hs: &mut HashSet<Move>| {
                        let gd = if p.is_white() {
                            GameData {
                                mask: gd.mask | GD_NO_WHITE_KS_CASTLE | GD_NO_WHITE_QS_CASTLE,
                                ..gd
                            }
                        } else {
                            GameData {
                                mask: gd.mask | GD_NO_BLACK_KS_CASTLE | GD_NO_BLACK_QS_CASTLE,
                                ..gd
                            }
                        };
                        add_linear_moves(p, pp, hs, &AXES, 1, gd);
                        add_linear_moves(p, pp, hs, &DIAGONALS, 1, gd);
                    },
                ),
            },
        );
        hm.insert(
            "kingside-castle",
            MovementRule {
                piece_constrait: Some('k'),
                f: Box::new(
                    |p: Piece, pp: &PiecePlacements, gd: GameData, hs: &mut HashSet<Move>| {
                        if p.is_white() {
                            if (gd.mask & GD_NO_WHITE_KS_CASTLE) != 0 {
                                return;
                            }
                        } else {
                            if (gd.mask & GD_NO_BLACK_KS_CASTLE) != 0 {
                                return;
                            }
                        }
                        let (ks, kd, rn, rs, rd, mask) = if p.is_white() {
                            (
                                (1, 5),
                                (1, 7),
                                'R' as u8,
                                (1, 8),
                                (1, 6),
                                GD_NO_WHITE_KS_CASTLE | GD_NO_WHITE_QS_CASTLE,
                            )
                        } else {
                            (
                                (8, 5),
                                (8, 7),
                                'r' as u8,
                                (8, 8),
                                (8, 6),
                                GD_NO_BLACK_KS_CASTLE | GD_NO_BLACK_QS_CASTLE,
                            )
                        };
                        // Make sure there's nothing between king and rook.
                        // FIXME: Make sure the king isn't in check, or castling through check.
                        // We don't really need to check the king starting location, since if the
                        // king has moved, no-castle flags would be set. But adding this check
                        // makes the tests more intuitive to write because we don't have to set
                        // no-castle flags on every test that involves the king.
                        if pp[ks.0][ks.1] != p.name || pp[kd.0][kd.1] != 0 || pp[rd.0][rd.1] != 0 {
                            return;
                        }
                        hs.insert(Move {
                            dst: Piece {
                                row: kd.0 as u8,
                                col: kd.1 as u8,
                                name: p.name,
                            },
                            typ: MoveType::Secondary {
                                src: Piece {
                                    row: rs.0 as u8,
                                    col: rs.1 as u8,
                                    name: rn,
                                },
                                dst: Piece {
                                    row: rd.0 as u8,
                                    col: rd.1 as u8,
                                    name: rn,
                                },
                            },
                            game_data: GameData {
                                mask: gd.mask | mask,
                                ..gd
                            },
                        });
                    },
                ),
            },
        );
        if !cfg!(test) {
            hm.insert(
                "js-plugin",
                MovementRule {
                    piece_constrait: None,
                    f: Box::new(
                        |p: Piece, pp: &PiecePlacements, gd: GameData, hs: &mut HashSet<Move>| {
                            plugin_movement_rule(p, pp, gd, hs)
                        },
                    ),
                },
            );
        }
        hm
    }

    pub fn allowed_moves(
        &self,
        piece: Piece,
        piece_placements: &PiecePlacements,
        gd: GameData,
    ) -> HashSet<Move> {
        let mut allowed: HashSet<Move> = HashSet::new();
        for (_, r) in self.movement_rules.iter() {
            if let Some(p) = r.piece_constrait && p.to_ascii_lowercase() != (piece.name as char).to_ascii_lowercase() {
                continue;
            }
            (r.f)(piece, piece_placements, gd, &mut allowed);
        }
        allowed
    }
}

fn std_in_bounds(r: i32, c: i32) -> bool {
    // TODO: Get bounds from rules
    1 <= r && r <= 8 && 1 <= c && c <= 8
}

fn plugin_movement_rule(p: Piece, pp: &PiecePlacements, gd: GameData, hs: &mut HashSet<Move>) {
    let piece_ptr: *const Piece = &p;
    let placements_ptr: *const [u8; 8 + 1] = pp.as_ptr();
    const RETVAL_LEN: usize = 3 * 8 * 8 * 95;
    let mut retval: [u8; RETVAL_LEN] = [0; RETVAL_LEN];
    let retval_ptr: *const u8 = retval.as_mut_ptr();
    unsafe {
        movement_plugin(
            piece_ptr as u32,
            placements_ptr as u32,
            retval_ptr as u32,
            RETVAL_LEN as u32,
        );
    }
    let mut i = 0;
    while i < RETVAL_LEN {
        if retval[i] == 0 {
            break;
        }
        let (r, c, n) = (retval[i] as usize, retval[i + 1] as usize, retval[i + 2]);
        if std_in_bounds(r as i32, c as i32) {
            if pp[r][c] != 0 {
                hs.insert(Move::capture(r, c, n, gd));
            } else {
                hs.insert(Move::normal(r, c, n, gd));
            }
        }
        i += 3;
    }
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
            let piece = Piece {
                row: 2,
                col,
                name: 'P' as u8,
            };
            let allowed = vec![
                Piece {
                    row: 3,
                    col,
                    name: 'P' as u8,
                },
                Piece {
                    row: 4,
                    col,
                    name: 'P' as u8,
                },
            ];
            assert_moves_allowed_eq(board, piece, allowed);
        }
        // Test black pieces, each column
        for col in 1..=8 {
            let piece = Piece {
                row: 7,
                col,
                name: 'p' as u8,
            };
            let allowed = vec![
                Piece {
                    row: 6,
                    col,
                    name: 'p' as u8,
                },
                Piece {
                    row: 5,
                    col,
                    name: 'p' as u8,
                },
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
        let piece = Piece {
            row: 3,
            col: 1,
            name: 'P' as u8,
        };
        let allowed = vec![Piece {
            row: 4,
            col: 1,
            name: 'P' as u8,
        }];
        assert_moves_allowed_eq(board, piece, allowed);
        // Black
        let piece = Piece {
            row: 6,
            col: 8,
            name: 'p' as u8,
        };
        let allowed = vec![Piece {
            row: 5,
            col: 8,
            name: 'p' as u8,
        }];
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
        let piece = Piece {
            row: 4,
            col: 1,
            name: 'P' as u8,
        };
        assert_moves_allowed_eq(board, piece, Vec::new());
        // Black
        let piece = Piece {
            row: 5,
            col: 1,
            name: 'p' as u8,
        };
        assert_moves_allowed_eq(board, piece, Vec::new());
    }

    #[test]
    fn test_pawn_captures() {
        let board = "
            rnbqkbnr
            ppp..ppp
            ........
            ...pp...
            ....P...
            ........
            PPPP.PPP
            RNBQKBNR
        ";
        // White
        let piece = Piece {
            row: 4,
            col: 5,
            name: 'P' as u8,
        };
        let allowed = vec![Piece {
            row: 5,
            col: 4,
            name: 'P' as u8,
        }];
        assert_moves_allowed_eq(board, piece, allowed);
        // Black
        let piece = Piece {
            row: 5,
            col: 4,
            name: 'p' as u8,
        };
        let allowed = vec![
            Piece {
                row: 4,
                col: 5,
                name: 'p' as u8,
            },
            Piece {
                row: 4,
                col: 4,
                name: 'p' as u8,
            },
        ];
        assert_moves_allowed_eq(board, piece, allowed);
    }

    #[test]
    fn test_bishop_moves() {
        let board = "
            ........
            ........
            ........
            ........
            ........
            ........
            ........
            B.....b.
        ";
        // White
        let piece = Piece {
            row: 1,
            col: 1,
            name: 'B' as u8,
        };
        let mut allowed = Vec::new();
        for i in 2..=8 {
            allowed.push(Piece {
                row: i,
                col: i,
                name: 'B' as u8,
            })
        }
        assert_moves_allowed_eq(board, piece, allowed);
        // Black
        let piece = Piece {
            row: 1,
            col: 7,
            name: 'b' as u8,
        };
        let allowed = vec![
            Piece {
                row: 2,
                col: 6,
                name: 'b' as u8,
            },
            Piece {
                row: 3,
                col: 5,
                name: 'b' as u8,
            },
            Piece {
                row: 4,
                col: 4,
                name: 'b' as u8,
            },
            Piece {
                row: 5,
                col: 3,
                name: 'b' as u8,
            },
            Piece {
                row: 6,
                col: 2,
                name: 'b' as u8,
            },
            Piece {
                row: 7,
                col: 1,
                name: 'b' as u8,
            },
            Piece {
                row: 2,
                col: 8,
                name: 'b' as u8,
            },
        ];
        assert_moves_allowed_eq(board, piece, allowed);
    }

    #[test]
    fn test_bishop_blocked_and_capture() {
        let board = "
            ........
            ........
            ........
            ........
            ........
            ........
            P.b.....
            .B......
        ";
        // White
        let piece = Piece {
            row: 1,
            col: 2,
            name: 'B' as u8,
        };
        let allowed = vec![Piece {
            row: 2,
            col: 3,
            name: 'B' as u8,
        }];
        assert_moves_allowed_eq(board, piece, allowed);
    }

    #[test]
    fn test_knight_moves() {
        let board = "
            ........
            ........
            ........
            ........
            ........
            ..N.....
            ........
            .......n
        ";
        // White
        let piece = Piece {
            row: 3,
            col: 3,
            name: 'N' as u8,
        };
        let allowed = vec![
            Piece {
                row: 5,
                col: 2,
                name: 'N' as u8,
            },
            Piece {
                row: 5,
                col: 4,
                name: 'N' as u8,
            },
            Piece {
                row: 2,
                col: 5,
                name: 'N' as u8,
            },
            Piece {
                row: 4,
                col: 5,
                name: 'N' as u8,
            },
            Piece {
                row: 1,
                col: 2,
                name: 'N' as u8,
            },
            Piece {
                row: 1,
                col: 4,
                name: 'N' as u8,
            },
            Piece {
                row: 2,
                col: 1,
                name: 'N' as u8,
            },
            Piece {
                row: 4,
                col: 1,
                name: 'N' as u8,
            },
        ];
        assert_moves_allowed_eq(board, piece, allowed);
        // Black
        let piece = Piece {
            row: 1,
            col: 8,
            name: 'n' as u8,
        };
        let allowed = vec![
            Piece {
                row: 3,
                col: 7,
                name: 'n' as u8,
            },
            Piece {
                row: 2,
                col: 6,
                name: 'n' as u8,
            },
        ];
        assert_moves_allowed_eq(board, piece, allowed);
    }

    #[test]
    fn test_knight_blocked_and_capture() {
        let board = "
            ........
            ........
            ........
            ........
            ........
            .N......
            ..n.....
            N.......
        ";
        // White
        let piece = Piece {
            row: 1,
            col: 1,
            name: 'N' as u8,
        };
        let allowed = vec![Piece {
            row: 2,
            col: 3,
            name: 'N' as u8,
        }];
        assert_moves_allowed_eq(board, piece, allowed);
    }

    #[test]
    fn test_rook() {
        let board = "
            ........
            ........
            ........
            ........
            .P......
            ........
            .R..p...
            ........
        ";
        // White
        let piece = Piece {
            row: 2,
            col: 2,
            name: 'R' as u8,
        };
        let allowed = vec![
            Piece {
                row: 3,
                col: 2,
                name: 'R' as u8,
            },
            Piece {
                row: 1,
                col: 2,
                name: 'R' as u8,
            },
            Piece {
                row: 2,
                col: 1,
                name: 'R' as u8,
            },
            Piece {
                row: 2,
                col: 3,
                name: 'R' as u8,
            },
            Piece {
                row: 2,
                col: 4,
                name: 'R' as u8,
            },
            Piece {
                row: 2,
                col: 5,
                name: 'R' as u8,
            },
        ];
        assert_moves_allowed_eq(board, piece, allowed);
    }

    #[test]
    fn test_queen() {
        let board = "
            ........
            ........
            ........
            ........
            .P......
            ........
            .Q..p...
            ........
        ";
        // White
        let piece = Piece {
            row: 2,
            col: 2,
            name: 'Q' as u8,
        };
        let allowed = vec![
            Piece {
                row: 3,
                col: 2,
                name: 'Q' as u8,
            },
            Piece {
                row: 3,
                col: 3,
                name: 'Q' as u8,
            },
            Piece {
                row: 4,
                col: 4,
                name: 'Q' as u8,
            },
            Piece {
                row: 5,
                col: 5,
                name: 'Q' as u8,
            },
            Piece {
                row: 6,
                col: 6,
                name: 'Q' as u8,
            },
            Piece {
                row: 7,
                col: 7,
                name: 'Q' as u8,
            },
            Piece {
                row: 8,
                col: 8,
                name: 'Q' as u8,
            },
            Piece {
                row: 1,
                col: 3,
                name: 'Q' as u8,
            },
            Piece {
                row: 1,
                col: 2,
                name: 'Q' as u8,
            },
            Piece {
                row: 1,
                col: 1,
                name: 'Q' as u8,
            },
            Piece {
                row: 2,
                col: 1,
                name: 'Q' as u8,
            },
            Piece {
                row: 3,
                col: 1,
                name: 'Q' as u8,
            },
            Piece {
                row: 2,
                col: 3,
                name: 'Q' as u8,
            },
            Piece {
                row: 2,
                col: 4,
                name: 'Q' as u8,
            },
            Piece {
                row: 2,
                col: 5,
                name: 'Q' as u8,
            },
        ];
        assert_moves_allowed_eq(board, piece, allowed);
    }

    #[test]
    fn test_king() {
        let board = "
            ........
            ........
            ........
            ........
            ........
            .P......
            .Kp.....
            ........
        ";
        // White
        let piece = Piece {
            row: 2,
            col: 2,
            name: 'K' as u8,
        };
        let allowed = vec![
            Piece {
                row: 3,
                col: 3,
                name: 'K' as u8,
            },
            Piece {
                row: 2,
                col: 3,
                name: 'K' as u8,
            },
            Piece {
                row: 1,
                col: 3,
                name: 'K' as u8,
            },
            Piece {
                row: 1,
                col: 2,
                name: 'K' as u8,
            },
            Piece {
                row: 1,
                col: 1,
                name: 'K' as u8,
            },
            Piece {
                row: 2,
                col: 1,
                name: 'K' as u8,
            },
            Piece {
                row: 3,
                col: 1,
                name: 'K' as u8,
            },
        ];
        assert_moves_allowed_eq(board, piece, allowed);
    }

    fn assert_moves_allowed_eq_with_gd(
        board: &str,
        piece: Piece,
        expect_allowed: Vec<Piece>,
        gd: GameData,
    ) {
        let expect_allowed: HashSet<Piece> = expect_allowed.into_iter().collect();
        let rules = Rules::defaults();
        let placements = string_board_to_placements(board);
        let allowed: HashSet<Piece> = rules
            .allowed_moves(piece, &placements, gd)
            .iter()
            .map(|m| m.dst)
            .collect();
        assert_eq!(allowed, expect_allowed);
    }

    fn assert_moves_allowed_eq(board: &str, piece: Piece, expect_allowed: Vec<Piece>) {
        assert_moves_allowed_eq_with_gd(board, piece, expect_allowed, GameData { ply: 1, mask: 0 });
    }

    fn string_board_to_placements(board: &str) -> PiecePlacements {
        let board = board.trim();
        let mut placements = [[0; 8 + 1]; 8 + 1];
        for (i, line) in board.split('\n').enumerate() {
            let r = 8 - i;
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
