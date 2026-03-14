use boltffi_ffi_rules::naming;
use heck::ToShoutySnakeCase;

pub struct NamingConvention;

impl NamingConvention {
    pub fn class_name(name: &str) -> String {
        naming::to_upper_camel_case(name)
    }

    pub fn method_name(name: &str) -> String {
        let converted = naming::snake_to_camel(name);
        Self::escape_keyword(&converted)
    }

    pub fn field_name(name: &str) -> String {
        let converted = naming::snake_to_camel(name);
        Self::escape_keyword(&converted)
    }

    pub fn enum_constant_name(name: &str) -> String {
        name.to_shouty_snake_case()
    }

    pub fn escape_keyword(name: &str) -> String {
        if Self::is_java_keyword(name) {
            format!("_{}", name)
        } else {
            name.to_string()
        }
    }

    fn is_java_keyword(name: &str) -> bool {
        matches!(
            name,
            "abstract"
                | "assert"
                | "boolean"
                | "break"
                | "byte"
                | "case"
                | "catch"
                | "char"
                | "class"
                | "const"
                | "continue"
                | "default"
                | "do"
                | "double"
                | "else"
                | "enum"
                | "extends"
                | "final"
                | "finally"
                | "float"
                | "for"
                | "goto"
                | "if"
                | "implements"
                | "import"
                | "instanceof"
                | "int"
                | "interface"
                | "long"
                | "native"
                | "new"
                | "package"
                | "private"
                | "protected"
                | "public"
                | "return"
                | "short"
                | "static"
                | "strictfp"
                | "super"
                | "switch"
                | "synchronized"
                | "this"
                | "throw"
                | "throws"
                | "transient"
                | "try"
                | "void"
                | "volatile"
                | "while"
                | "true"
                | "false"
                | "null"
        )
    }

    pub fn ffi_prefix() -> String {
        naming::ffi_prefix().to_string()
    }

    pub fn class_ffi_prefix(class_name: &str) -> String {
        naming::class_ffi_prefix(class_name).into_string()
    }
}
