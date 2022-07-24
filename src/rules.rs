use std::{
    cmp::{max, min},
    collections::{HashMap, HashSet},
};

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
// FIXME: need to be able to remove a piece on a different square than where the piece moves
//        for en passant
pub trait MovementRuleFn = Fn(Piece, &PiecePlacements, GameData, &mut HashSet<Move>);
pub trait ConstraintRuleFn = Fn(Piece, &PiecePlacements, GameData) -> bool;

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
    // Key: rule name. Value: a callable that (dis)allows a move (for, leaves king in check).
    pub move_constraint_rules: HashMap<&'a str, Box<dyn ConstraintRuleFn>>,
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

fn add_knight_moves(p: Piece, pp: &PiecePlacements, hs: &mut HashSet<Move>, gd: GameData) {
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
}

fn add_pawn_captures(p: Piece, pp: &PiecePlacements, hs: &mut HashSet<Move>, gd: GameData) {
    let dir: i8 = if p.is_white() { 1 } else { -1 };
    for i in [-1, 1] {
        let r = (p.row as i8 + dir) as usize;
        let c = (p.col as i8 + i) as usize;
        if 1 <= c && c <= 8 && pp[r][c] != 0 && is_piece_white(pp[r][c]) != p.is_white() {
            hs.insert(Move::capture(r, c, p.name, gd));
        }
    }
}

fn piece_attacked(p: Piece, pp: &PiecePlacements, game_data: GameData) -> bool {
    let gd = GameData {
        mask: GD_NO_BLACK_KS_CASTLE
            | GD_NO_BLACK_QS_CASTLE
            | GD_NO_WHITE_KS_CASTLE
            | GD_NO_WHITE_QS_CASTLE,
        ..game_data
    };
    let white = p.is_white();
    let mut hs = HashSet::<Move>::new();
    // TODO: Turn these into fn so I don't need to box them.
    let gen_rook_attacks: Box<dyn Fn(&mut HashSet<Move>)> = Box::new(|hs: &mut HashSet<Move>| {
        add_linear_moves(
            Piece {
                name: if white { 'R' } else { 'r' } as u8,
                ..p
            },
            pp,
            hs,
            &AXES,
            8,
            gd,
        );
    });
    let gen_bishop_attacks: Box<dyn Fn(&mut HashSet<Move>)> = Box::new(|hs: &mut HashSet<Move>| {
        add_linear_moves(
            Piece {
                name: if white { 'B' } else { 'b' } as u8,
                ..p
            },
            pp,
            hs,
            &DIAGONALS,
            8,
            gd,
        );
    });
    let gen_knight_attacks: Box<dyn Fn(&mut HashSet<Move>)> = Box::new(|hs: &mut HashSet<Move>| {
        add_knight_moves(
            Piece {
                name: if white { 'N' } else { 'n' } as u8,
                ..p
            },
            pp,
            hs,
            gd,
        );
    });
    let gen_pawn_attacks: Box<dyn Fn(&mut HashSet<Move>)> = Box::new(|hs: &mut HashSet<Move>| {
        add_pawn_captures(
            Piece {
                name: if white { 'P' } else { 'p' } as u8,
                ..p
            },
            pp,
            hs,
            gd,
        );
    });
    // We could optimize king attacks by checking if the opponent king is within
    // one square. But for simplicity will do this for now.
    let gen_king_attacks: Box<dyn Fn(&mut HashSet<Move>)> = Box::new(|hs: &mut HashSet<Move>| {
        add_linear_moves(
            Piece {
                name: if white { 'K' } else { 'k' } as u8,
                ..p
            },
            pp,
            hs,
            &AXES,
            1,
            gd,
        );
        add_linear_moves(
            Piece {
                name: if white { 'K' } else { 'k' } as u8,
                ..p
            },
            pp,
            hs,
            &DIAGONALS,
            1,
            gd,
        );
    });
    let moves_to_gen = [
        (gen_rook_attacks, "RQ"),
        (gen_bishop_attacks, "BQ"),
        (gen_knight_attacks, "N"),
        (gen_pawn_attacks, "P"),
        (gen_king_attacks, "K"),
    ];
    for (f, pieces) in moves_to_gen {
        hs.clear();
        f(&mut hs);
        for m in hs.iter() {
            if let MoveType::Capture { row, col } = m.typ {
                let n = (pp[row as usize][col as usize] as char).to_ascii_uppercase();
                for piece in pieces.chars() {
                    if n == piece {
                        return true;
                    }
                }
            }
        }
    }
    return false;
}

fn add_castle(
    p: Piece,
    pp: &PiecePlacements,
    gd: GameData,
    hs: &mut HashSet<Move>,
    rook_col: usize,
) {
    let mask = if p.is_white() {
        if rook_col == 1 {
            GD_NO_WHITE_QS_CASTLE
        } else {
            GD_NO_WHITE_KS_CASTLE
        }
    } else {
        if rook_col == 1 {
            GD_NO_BLACK_QS_CASTLE
        } else {
            GD_NO_BLACK_KS_CASTLE
        }
    };
    if (gd.mask & mask) != 0 {
        return;
    }
    let (row, new_mask, rn) = if p.is_white() {
        (1, GD_NO_WHITE_KS_CASTLE | GD_NO_WHITE_QS_CASTLE, 'R' as u8)
    } else {
        (8, GD_NO_BLACK_KS_CASTLE | GD_NO_BLACK_QS_CASTLE, 'r' as u8)
    };
    let ks = 5; // King starting square
    let (kd, rd) = if rook_col == 1 {
        // King / rook destination squares
        (3, 4)
    } else {
        (7, 6)
    };

    // We don't really need to check the king starting location, since if the
    // king has moved, no-castle flags would be set. But adding this check
    // makes the tests more intuitive to write because we don't have to set
    // no-castle flags on every test that involves the king.
    if pp[row][ks] != p.name || pp[row][rook_col] != rn {
        return;
    }

    // Make sure the king isn't castling while in check.
    if piece_attacked(
        Piece {
            row: row as u8,
            col: ks as u8,
            name: p.name,
        },
        pp,
        gd,
    ) {
        return;
    }

    // Make sure there's nothing between king and rook.
    for col in min(rook_col, ks) + 1..=max(rook_col, ks) - 1 {
        if pp[row][col] != 0
            || piece_attacked(
                Piece {
                    row: row as u8,
                    col: col as u8,
                    name: p.name,
                },
                pp,
                gd,
            )
        {
            return;
        }
    }
    // FIXME: Make sure the king isn't in check, or castling through check.
    hs.insert(Move {
        dst: Piece {
            row: row as u8,
            col: kd,
            name: p.name,
        },
        typ: MoveType::Secondary {
            src: Piece {
                row: row as u8,
                col: rook_col as u8,
                name: rn,
            },
            dst: Piece {
                row: row as u8,
                col: rd,
                name: rn,
            },
        },
        game_data: GameData {
            mask: gd.mask | new_mask,
            ..gd
        },
    });
}

fn find_piece(name: char, pp: &PiecePlacements) -> Option<(u8, u8)> {
    let name = name as u8;
    for r in 0..8 {
        // TODO: get board size from rules
        for c in 0..8 {
            if pp[r][c] == name {
                return Some((r as u8, c as u8));
            }
        }
    }
    None
}

impl<'a> Rules<'a> {
    pub fn defaults() -> Self {
        Self {
            piece_name_to_offsets: Self::default_piece_name_to_offsets(),
            setup_rules: Self::default_setup_rules(),
            turn_rules: Self::default_turn_rules(),
            movement_rules: Self::default_movement_rules(),
            move_constraint_rules: Self::default_move_constraint_rules(),
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
                        add_pawn_captures(p, pp, hs, gd);
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
                        add_knight_moves(p, pp, hs, gd);
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
                        add_castle(p, pp, gd, hs, 8);
                    },
                ),
            },
        );
        hm.insert(
            "queenside-castle",
            MovementRule {
                piece_constrait: Some('k'),
                f: Box::new(
                    |p: Piece, pp: &PiecePlacements, gd: GameData, hs: &mut HashSet<Move>| {
                        add_castle(p, pp, gd, hs, 1);
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

    fn default_move_constraint_rules() -> HashMap<&'a str, Box<dyn ConstraintRuleFn>> {
        let mut hm = HashMap::<&'a str, Box<dyn ConstraintRuleFn>>::new();
        hm.insert(
            "resolve-check",
            Box::new(|p: Piece, pp: &PiecePlacements, gd: GameData| {
                let king = if p.is_white() { 'K' } else { 'k' };
                if let Some((r, c)) = find_piece(king, pp) {
                    let kp = Piece {
                        row: r,
                        col: c,
                        name: king as u8,
                    };
                    return !piece_attacked(kp, pp, gd);
                }
                true
            }),
        );
        hm
    }

    pub fn make_move(piece: Piece, m: Move, piece_placements: &mut PiecePlacements) {
        let (sr, sc) = (piece.row as usize, piece.col as usize);
        let (r, c) = (m.dst.row as usize, m.dst.col as usize);
        piece_placements[sr][sc] = 0;
        piece_placements[r][c] = piece.name;
        match m.typ {
            MoveType::Capture { row: cr, col: cc } => {
                if (cr as usize, cc as usize) != (r, c) {
                    piece_placements[cr as usize][cc as usize] = 0;
                }
            }
            MoveType::Secondary { src: ss, dst: sd } => {
                if (ss.row as usize, ss.col as usize) != (r, c) {
                    piece_placements[ss.row as usize][ss.col as usize] = 0;
                }
                piece_placements[sd.row as usize][sd.col as usize] = sd.name;
            }
            MoveType::Normal => {}
        }
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
        self.constrain_moves(&allowed, piece, piece_placements, gd)
    }

    fn constrain_moves(
        &self,
        hs: &HashSet<Move>,
        p: Piece,
        pp: &PiecePlacements,
        gd: GameData,
    ) -> HashSet<Move> {
        let mut post_pp = pp.clone();
        let (sr, sc) = (p.row as usize, p.col as usize);
        hs.iter()
            .filter(|&&m| {
                let mut allow = true;
                let (dr, dc) = (m.dst.row as usize, m.dst.col as usize);
                // Make the move
                Rules::make_move(p, m, &mut post_pp);
                for (_, r) in self.move_constraint_rules.iter() {
                    if !r(p, &post_pp, gd) {
                        allow = false;
                        break;
                    }
                }
                // Reset the board
                post_pp[sr][sc] = pp[sr][sc];
                post_pp[dr][dc] = pp[dr][dc];
                if let MoveType::Capture { row, col } = m.typ {
                    let (cr, cc) = (row as usize, col as usize);
                    post_pp[cr][cc] = pp[cr][cc];
                } else if let MoveType::Secondary { src, dst } = m.typ {
                    let (ssr, ssc) = (src.row as usize, src.col as usize);
                    let (sdr, sdc) = (dst.row as usize, dst.col as usize);
                    post_pp[ssr][ssc] = pp[ssr][ssc];
                    post_pp[sdr][sdc] = pp[sdr][sdc];
                }
                allow
            })
            .copied()
            .collect()
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
            assert_moves_allowed_eq(board, piece, &allowed);
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
            assert_moves_allowed_eq(board, piece, &allowed);
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
        assert_moves_allowed_eq(board, piece, &allowed);
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
        assert_moves_allowed_eq(board, piece, &allowed);
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
        assert_moves_allowed_eq(board, piece, &Vec::new());
        // Black
        let piece = Piece {
            row: 5,
            col: 1,
            name: 'p' as u8,
        };
        assert_moves_allowed_eq(board, piece, &Vec::new());
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
        assert_moves_allowed_eq(board, piece, &allowed);
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
        assert_moves_allowed_eq(board, piece, &allowed);
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
        assert_moves_allowed_eq(board, piece, &allowed);
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
        assert_moves_allowed_eq(board, piece, &allowed);
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
        assert_moves_allowed_eq(board, piece, &allowed);
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
        assert_moves_allowed_eq(board, piece, &allowed);
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
        assert_moves_allowed_eq(board, piece, &allowed);
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
        assert_moves_allowed_eq(board, piece, &allowed);
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
        assert_moves_allowed_eq(board, piece, &allowed);
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
        assert_moves_allowed_eq(board, piece, &allowed);
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
        assert_moves_allowed_eq(board, piece, &allowed);
    }

    #[test]
    fn test_castles_kingside() {
        let board = "
            ........
            ........
            ........
            ........
            ........
            ........
            ........
            ....K..R
        ";
        // White
        let piece = Piece {
            row: 1,
            col: 5,
            name: 'K' as u8,
        };
        let mut allowed = vec![
            Piece {
                row: 1,
                col: 4,
                name: 'K' as u8,
            },
            Piece {
                row: 2,
                col: 4,
                name: 'K' as u8,
            },
            Piece {
                row: 2,
                col: 5,
                name: 'K' as u8,
            },
            Piece {
                row: 2,
                col: 6,
                name: 'K' as u8,
            },
            Piece {
                row: 1,
                col: 6,
                name: 'K' as u8,
            },
        ];
        let gd = GameData {
            ply: 1,
            mask: GD_NO_WHITE_KS_CASTLE,
        };
        assert_moves_allowed_eq_with_gd(board, piece, &allowed, gd);

        allowed.push(Piece {
            row: 1,
            col: 7,
            name: 'K' as u8,
        });
        assert_moves_allowed_eq(board, piece, &allowed);
    }

    #[test]
    fn test_castles_queenside() {
        let board = "
            r...kq..
            ...ppp..
            ........
            ........
            ........
            ........
            ........
            ........
        ";
        let piece = Piece {
            row: 8,
            col: 5,
            name: 'k' as u8,
        };
        let mut allowed = vec![Piece {
            row: 8,
            col: 4,
            name: 'k' as u8,
        }];
        let gd = GameData {
            ply: 1,
            mask: GD_NO_BLACK_QS_CASTLE,
        };
        assert_moves_allowed_eq_with_gd(board, piece, &allowed, gd);

        allowed.push(Piece {
            row: 8,
            col: 3,
            name: 'k' as u8,
        });
        assert_moves_allowed_eq(board, piece, &allowed);
    }

    #[test]
    fn test_castles_through_piece() {
        let board = "
            r..qkb..
            ...ppp..
            ........
            ........
            ........
            ........
            ........
            ........
        ";
        let piece = Piece {
            row: 8,
            col: 5,
            name: 'k' as u8,
        };
        assert_moves_allowed_eq(board, piece, &Vec::new());
    }

    #[test]
    fn test_castles_through_check() {
        let piece = Piece {
            row: 1,
            col: 5,
            name: 'K' as u8,
        };
        let allowed = vec![
            // Castles not allowed.
            Piece {
                row: 1,
                col: 4,
                name: 'K' as u8,
            },
            Piece {
                row: 2,
                col: 4,
                name: 'K' as u8,
            },
            Piece {
                row: 2,
                col: 5,
                name: 'K' as u8,
            },
            Piece {
                row: 2,
                col: 6,
                name: 'K' as u8,
            },
            Piece {
                row: 1,
                col: 6,
                name: 'K' as u8,
            },
        ];
        let board = "
            ......q.
            ........
            ........
            ........
            ........
            ........
            ........
            ....K..R
        ";
        assert_moves_allowed_eq(board, piece, &allowed);

        let board = "
            ......r.
            ........
            ........
            ........
            ........
            ........
            ........
            ....K..R
        ";
        assert_moves_allowed_eq(board, piece, &allowed);

        let board = "
            ........
            ........
            ........
            ........
            ........
            q.......
            ........
            R...K...
        ";
        assert_moves_allowed_eq(board, piece, &allowed);

        let board = "
            ........
            ........
            ........
            ........
            ........
            ........
            .......b
            ....K..R
        ";
        assert_moves_allowed_eq(board, piece, &allowed);

        let board = "
            ........
            ........
            ........
            ........
            ........
            ........
            n.......
            R...K...
        ";
        assert_moves_allowed_eq(board, piece, &allowed);

        let board = "
            ........
            ........
            ........
            ........
            ........
            ........
            .......p
            ....K..R
        ";
        assert_moves_allowed_eq(board, piece, &allowed);

        let board = "
            ........
            ........
            ........
            ........
            ........
            ........
            .......k
            ....K..R
        ";
        assert_moves_allowed_eq(board, piece, &allowed);
    }

    fn assert_moves_allowed_eq_with_gd(
        board: &str,
        piece: Piece,
        expect_allowed: &Vec<Piece>,
        gd: GameData,
    ) {
        let expect_allowed: HashSet<Piece> = expect_allowed.iter().map(|&p| p).collect();
        let rules = Rules::defaults();
        let placements = string_board_to_placements(board);
        let allowed: HashSet<Piece> = rules
            .allowed_moves(piece, &placements, gd)
            .iter()
            .map(|m| m.dst)
            .collect();
        assert_eq!(allowed, expect_allowed);
    }

    fn assert_moves_allowed_eq(board: &str, piece: Piece, expect_allowed: &Vec<Piece>) {
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
