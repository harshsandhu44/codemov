#[cfg(test)]
mod rust_tests {
    use crate::rust::extract;
    use codemov_core::SymbolKind;

    #[test]
    fn extracts_fn_struct_enum_trait_impl() {
        let src = r#"
pub fn hello() {}
pub struct Foo { x: i32 }
pub enum Bar { A, B }
pub trait Baz { fn method(&self); }
impl Foo { pub fn new() -> Self { Foo { x: 0 } } }
"#;
        let syms = extract(src.as_bytes()).unwrap();
        let kinds: Vec<_> = syms.iter().map(|s| s.kind).collect();
        let names: Vec<_> = syms.iter().map(|s| s.name.as_str()).collect();

        assert!(names.contains(&"hello"), "missing fn hello");
        assert!(names.contains(&"Foo"), "missing struct Foo");
        assert!(names.contains(&"Bar"), "missing enum Bar");
        assert!(names.contains(&"Baz"), "missing trait Baz");
        assert!(kinds.contains(&SymbolKind::Impl), "missing impl");
    }

    #[test]
    fn nested_fn_not_at_top_level() {
        let src = r#"
pub fn outer() {
    fn inner() {}
}
"#;
        let syms = extract(src.as_bytes()).unwrap();
        let names: Vec<_> = syms.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"outer"));
        assert!(names.contains(&"inner"));
    }

    #[test]
    fn line_numbers_are_one_based() {
        let src = "pub fn first() {}\npub fn second() {}\n";
        let syms = extract(src.as_bytes()).unwrap();
        assert_eq!(syms[0].start_line, 1);
        assert_eq!(syms[1].start_line, 2);
    }
}

#[cfg(test)]
mod ts_tests {
    use crate::typescript::extract;
    use codemov_core::{Language, SymbolKind};

    #[test]
    fn extracts_function_class_interface_type() {
        let src = r#"
function greet(name: string): void {}
class Animal { constructor() {} }
interface Shape { area(): number; }
type Color = "red" | "blue";
"#;
        let syms = extract(src.as_bytes(), Language::TypeScript).unwrap();
        let names: Vec<_> = syms.iter().map(|s| s.name.as_str()).collect();
        let kinds: Vec<_> = syms.iter().map(|s| s.kind).collect();

        assert!(names.contains(&"greet"));
        assert!(names.contains(&"Animal"));
        assert!(names.contains(&"Shape"));
        assert!(names.contains(&"Color"));
        assert!(kinds.contains(&SymbolKind::TypeAlias));
        assert!(kinds.contains(&SymbolKind::Interface));
    }

    #[test]
    fn extracts_arrow_function_export() {
        let src = "export const handler = async (req: any) => {};\n";
        let syms = extract(src.as_bytes(), Language::TypeScript).unwrap();
        let names: Vec<_> = syms.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"handler"), "missing exported arrow fn");
    }
}
