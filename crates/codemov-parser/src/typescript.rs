use codemov_core::{Language, Symbol, SymbolKind};
use tree_sitter::Node;

use crate::ParseError;

pub fn extract(source: &[u8], language: Language) -> Result<Vec<Symbol>, ParseError> {
    let mut parser = tree_sitter::Parser::new();
    let lang = match language {
        Language::TypeScript => tree_sitter_typescript::language_typescript(),
        _ => tree_sitter_typescript::language_tsx(),
    };
    parser
        .set_language(&lang)
        .map_err(|e| ParseError::Parse(e.to_string()))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| ParseError::Parse("tree-sitter returned None".into()))?;

    let mut symbols = Vec::new();
    walk(tree.root_node(), source, false, &mut symbols);
    Ok(symbols)
}

fn walk(node: Node, source: &[u8], inside_export: bool, out: &mut Vec<Symbol>) {
    match node.kind() {
        "function_declaration" | "function" | "generator_function_declaration" => {
            if let Some(sym) = named(node, source, SymbolKind::Function, "name") {
                out.push(sym);
                return; // don't recurse into function body for top-level symbols
            }
        }
        "class_declaration" | "abstract_class_declaration" => {
            if let Some(sym) = named(node, source, SymbolKind::Class, "name") {
                out.push(sym);
                return;
            }
        }
        "interface_declaration" => {
            if let Some(sym) = named(node, source, SymbolKind::Interface, "name") {
                out.push(sym);
                return;
            }
        }
        "type_alias_declaration" => {
            if let Some(sym) = named(node, source, SymbolKind::TypeAlias, "name") {
                out.push(sym);
                return;
            }
        }
        "lexical_declaration" | "variable_declaration" => {
            extract_variable_decl(node, source, inside_export, out);
            return;
        }
        "export_statement" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                walk(child, source, true, out);
            }
            return;
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, inside_export, out);
    }
}

fn extract_variable_decl(node: Node, source: &[u8], inside_export: bool, out: &mut Vec<Symbol>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            if let Some(name_node) = child.child_by_field_name("name") {
                let value = child.child_by_field_name("value");
                let kind = match value.map(|v| v.kind()) {
                    Some("arrow_function") | Some("function") => SymbolKind::Function,
                    _ if inside_export => SymbolKind::Export,
                    _ => continue,
                };
                if let Ok(name) = name_node.utf8_text(source) {
                    out.push(Symbol {
                        name: name.to_string(),
                        kind,
                        start_line: node.start_position().row as u32 + 1,
                        end_line: node.end_position().row as u32 + 1,
                    });
                }
            }
        }
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
