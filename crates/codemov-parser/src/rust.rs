use codemov_core::{ImportEdge, ImportKind, Symbol, SymbolKind};
use tree_sitter::Node;

use crate::ParseError;

pub fn extract(source: &[u8]) -> Result<Vec<Symbol>, ParseError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::language())
        .map_err(|e| ParseError::Parse(e.to_string()))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| ParseError::Parse("tree-sitter returned None".into()))?;

    let mut symbols = Vec::new();
    walk(tree.root_node(), source, &mut symbols);
    Ok(symbols)
}

fn walk(node: Node, source: &[u8], out: &mut Vec<Symbol>) {
    match node.kind() {
        "function_item" => {
            if let Some(sym) = named(node, source, SymbolKind::Function, "name") {
                out.push(sym);
            }
        }
        "struct_item" => {
            if let Some(sym) = named(node, source, SymbolKind::Struct, "name") {
                out.push(sym);
            }
        }
        "enum_item" => {
            if let Some(sym) = named(node, source, SymbolKind::Enum, "name") {
                out.push(sym);
            }
        }
        "trait_item" => {
            if let Some(sym) = named(node, source, SymbolKind::Trait, "name") {
                out.push(sym);
            }
        }
        "impl_item" => {
            let name = node
                .child_by_field_name("type")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("_")
                .to_string();
            out.push(Symbol {
                name,
                kind: SymbolKind::Impl,
                start_line: node.start_position().row as u32 + 1,
                end_line: node.end_position().row as u32 + 1,
            });
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, out);
    }
}

pub fn extract_imports(source: &[u8]) -> Result<Vec<ImportEdge>, crate::ParseError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::language())
        .map_err(|e| crate::ParseError::Parse(e.to_string()))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| crate::ParseError::Parse("tree-sitter returned None".into()))?;

    let mut edges = Vec::new();
    collect_use_decls(tree.root_node(), source, &mut edges);
    Ok(edges)
}

fn collect_use_decls(node: Node, source: &[u8], out: &mut Vec<ImportEdge>) {
    if node.kind() == "use_declaration" {
        if let Ok(text) = node.utf8_text(source) {
            // Strip leading "use " and trailing ";"
            let raw = text
                .trim()
                .strip_prefix("use ")
                .unwrap_or(text.trim())
                .trim_end_matches(';')
                .trim()
                .to_string();
            out.push(ImportEdge {
                source_path: std::path::PathBuf::new(),
                target_raw: raw,
                kind: ImportKind::Use,
                line: node.start_position().row as u32 + 1,
            });
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_use_decls(child, source, out);
    }
}

fn named(node: Node, source: &[u8], kind: SymbolKind, field: &str) -> Option<Symbol> {
    let name = node
        .child_by_field_name(field)?
        .utf8_text(source)
        .ok()?
        .to_string();
    Some(Symbol {
        name,
        kind,
        start_line: node.start_position().row as u32 + 1,
        end_line: node.end_position().row as u32 + 1,
    })
}
