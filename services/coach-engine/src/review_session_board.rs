use crate::review_session_contract::{
    Color, Piece, PieceRole, PositionRef, PositionSnapshot, Square,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphicalBoard {
    pub position_ref: PositionRef,
    pub squares: Vec<GraphicalBoardSquare>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphicalBoardSquare {
    pub square: Square,
    pub piece: Option<Piece>,
}

pub fn graphical_board(position: &PositionSnapshot) -> GraphicalBoard {
    let squares = (b'1'..=b'8')
        .rev()
        .flat_map(|rank| {
            (b'a'..=b'h').map(move |file| {
                let name = format!("{}{}", char::from(file), char::from(rank));
                let square = Square::try_from(name).expect("generated board square is valid");
                let piece = position
                    .occupied
                    .iter()
                    .find(|occupied| occupied.square == square)
                    .map(|occupied| occupied.piece);
                GraphicalBoardSquare { square, piece }
            })
        })
        .collect();
    GraphicalBoard {
        position_ref: position.position_ref.clone(),
        squares,
    }
}

pub fn coordinate_text_board(position: &PositionSnapshot) -> String {
    let board = graphical_board(position);
    let mut lines = board
        .squares
        .chunks_exact(8)
        .map(|rank| {
            let rank_label = rank[0]
                .square
                .as_str()
                .chars()
                .nth(1)
                .expect("a Square always has a rank");
            let pieces = rank
                .iter()
                .map(|square| square.piece.map_or('.', piece_symbol))
                .flat_map(|symbol| [symbol, ' '])
                .collect::<String>();
            format!("{rank_label} | {}", pieces.trim_end())
        })
        .collect::<Vec<_>>();
    lines.push("  +----------------".to_string());
    lines.push("    a b c d e f g h".to_string());
    lines.push(format!(
        "Side to move: {}",
        match position.side_to_move {
            Color::White => "White",
            Color::Black => "Black",
        }
    ));
    lines.join("\n")
}

fn piece_symbol(piece: Piece) -> char {
    let symbol = match piece.role {
        PieceRole::Pawn => 'p',
        PieceRole::Knight => 'n',
        PieceRole::Bishop => 'b',
        PieceRole::Rook => 'r',
        PieceRole::Queen => 'q',
        PieceRole::King => 'k',
    };
    match piece.color {
        Color::White => symbol.to_ascii_uppercase(),
        Color::Black => symbol,
    }
}
