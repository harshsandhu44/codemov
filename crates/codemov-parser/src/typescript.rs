use codemov_core::{ImportEdge, ImportKind, Language, Symbol, SymbolKind};
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

pub fn extract_imports(
    source: &[u8],
    language: Language,
) -> Result<Vec<ImportEdge>, crate::ParseError> {
    let mut parser = tree_sitter::Parser::new();
    let lang = match language {
        Language::TypeScript => tree_sitter_typescript::language_typescript(),
        _ => tree_sitter_typescript::language_tsx(),
    };
    parser
        .set_language(&lang)
        .map_err(|e| crate::ParseError::Parse(e.to_string()))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| crate::ParseError::Parse("tree-sitter returned None".into()))?;

    let mut edges = Vec::new();
    collect_import_nodes(tree.root_node(), source, &mut edges);
    Ok(edges)
}

fn collect_import_nodes(node: Node, source: &[u8], out: &mut Vec<ImportEdge>) {
    match node.kind() {
        "import_statement" => {
            if let Some(src_node) = node.child_by_field_name("source") {
                if let Ok(raw) = src_node.utf8_text(source) {
                    let target = raw.trim_matches(|c| c == '\'' || c == '"').to_string();
                    out.push(ImportEdge {
                        source_path: std::path::PathBuf::new(),
                        target_raw: target,
                        kind: ImportKind::Import,
                        line: node.start_position().row as u32 + 1,
                    });
                }
            }
        }
        "export_statement" => {
            // re-exports: export { ... } from "..."
            if let Some(src_node) = node.child_by_field_name("source") {
                if let Ok(raw) = src_node.utf8_text(source) {
                    let target = raw.trim_matches(|c| c == '\'' || c == '"').to_string();
                    out.push(ImportEdge {
                        source_path: std::path::PathBuf::new(),
                        target_raw: target,
                        kind: ImportKind::Export,
                        line: node.start_position().row as u32 + 1,
                    });
                }
            }
        }
        "call_expression" => {
            // require("...") calls
            if let Some(fn_node) = node.child_by_field_name("function") {
                if fn_node.utf8_text(source).ok() == Some("require") {
                    if let Some(args) = node.child_by_field_name("arguments") {
                        let mut cur = args.walk();
                        for child in args.children(&mut cur) {
                            if matches!(child.kind(), "string" | "template_string") {
                                if let Ok(raw) = child.utf8_text(source) {
                                    let target =
                                        raw.trim_matches(|c| c == '\'' || c == '"').to_string();
                                    out.push(ImportEdge {
                                        source_path: std::path::PathBuf::new(),
                                        target_raw: target,
                                        kind: ImportKind::Require,
                                        line: node.start_position().row as u32 + 1,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_import_nodes(child, source, out);
    }
}
