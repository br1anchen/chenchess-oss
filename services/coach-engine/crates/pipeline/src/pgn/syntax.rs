use tree_sitter::{Node, Parser};

pub(super) struct SyntaxGame {
    pub tags: Vec<(String, String)>,
    pub moves: Vec<String>,
}

pub(super) fn parse_single_game(input: &str) -> Result<SyntaxGame, String> {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_pgn::LANGUAGE.into())
        .map_err(|error| format!("could not load PGN grammar: {error}"))?;
    let tree = parser
        .parse(input, None)
        .ok_or_else(|| "could not parse PGN".to_string())?;
    let root = tree.root_node();

    if root.has_error() {
        return Err("PGN contains invalid syntax".to_string());
    }

    let mut cursor = root.walk();
    let games = root
        .named_children(&mut cursor)
        .filter(|node| node.kind() == "game")
        .collect::<Vec<_>>();
    let game = match games.as_slice() {
        [game] => *game,
        [] => return Err("PGN does not contain a game".to_string()),
        _ => return Err("PGN contains multiple games; paste one game at a time".to_string()),
    };

    if has_named_child(game, "trailing_garbage") {
        return Err("PGN contains invalid trailing movetext".to_string());
    }

    Ok(SyntaxGame {
        tags: parse_tags(game, input.as_bytes())?,
        moves: parse_mainline_moves(game, input.as_bytes())?,
    })
}

fn parse_tags(game: Node<'_>, source: &[u8]) -> Result<Vec<(String, String)>, String> {
    let Some(header) = game.child_by_field_name("header") else {
        return Ok(Vec::new());
    };
    let mut cursor = header.walk();
    header
        .named_children(&mut cursor)
        .filter(|node| node.kind() == "tagpair")
        .map(|tagpair| {
            let key = node_text(
                tagpair
                    .child_by_field_name("tagpair_key")
                    .ok_or_else(|| "PGN tag is missing a key".to_string())?,
                source,
            )?;
            let value = tagpair
                .child_by_field_name("tagpair_value_contents")
                .map(|node| node_text(node, source))
                .transpose()?
                .unwrap_or_default();
            Ok((key.to_string(), unescape_tag_value(value)))
        })
        .collect()
}

fn parse_mainline_moves(game: Node<'_>, source: &[u8]) -> Result<Vec<String>, String> {
    let Some(movetext) = game.child_by_field_name("movetext") else {
        return Ok(Vec::new());
    };
    let mut cursor = movetext.walk();
    movetext
        .named_children(&mut cursor)
        .filter(|node| matches!(node.kind(), "san_move" | "lan_move"))
        .map(|node| node_text(node, source).map(|text| text.trim().to_string()))
        .collect()
}

fn has_named_child(node: Node<'_>, kind: &str) -> bool {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .any(|child| child.kind() == kind);
    found
}

fn node_text<'a>(node: Node<'_>, source: &'a [u8]) -> Result<&'a str, String> {
    node.utf8_text(source)
        .map_err(|_| "PGN contains invalid UTF-8".to_string())
}

fn unescape_tag_value(value: &str) -> String {
    let mut unescaped = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(escaped) = chars.next() {
                unescaped.push(escaped);
            }
        } else {
            unescaped.push(ch);
        }
    }
    unescaped
}
