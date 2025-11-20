use heck::{ToLowerCamelCase, ToUpperCamelCase};

pub struct NamingConvention;

impl NamingConvention {
    pub fn class_name(name: &str) -> String {
        name.to_upper_camel_case()
    }

    pub fn method_name(name: &str) -> String {
        let converted = name.to_lower_camel_case();
        Self::escape_keyword(&converted)
    }

    pub fn param_name(name: &str) -> String {
        let converted = name.to_lower_camel_case();
        Self::escape_keyword(&converted)
    }

    pub fn property_name(name: &str) -> String {
        let converted = name.to_lower_camel_case();
        Self::escape_keyword(&converted)
    }

    pub fn enum_case_name(name: &str) -> String {
        let converted = name.to_lower_camel_case();
        Self::escape_keyword(&converted)
    }

    pub fn escape_keyword(name: &str) -> String {
        if Self::is_swift_keyword(name) {
            format!("`{}`", name)
        } else {
            name.to_string()
        }
    }

    pub fn is_swift_keyword(name: &str) -> bool {
        matches!(
            name,
            "associatedtype"
                | "class"
                | "deinit"
                | "enum"
                | "extension"
                | "fileprivate"
                | "func"
                | "import"
                | "init"
                | "inout"
                | "internal"
                | "let"
                | "open"
                | "operator"
                | "private"
                | "precedencegroup"
                | "protocol"
                | "public"
                | "rethrows"
                | "static"
                | "struct"
                | "subscript"
                | "typealias"
                | "var"
                | "break"
                | "case"
                | "catch"
                | "continue"
                | "default"
                | "defer"
                | "do"
                | "else"
                | "fallthrough"
                | "for"
                | "guard"
                | "if"
                | "in"
                | "repeat"
                | "return"
                | "throw"
                | "switch"
                | "where"
                | "while"
                | "Any"
                | "as"
                | "await"
                | "false"
                | "is"
                | "nil"
                | "self"
                | "Self"
                | "super"
                | "throws"
                | "true"
                | "try"
                | "Type"
        )
    }

    pub fn ffi_prefix(module_name: &str) -> String {
        format!("riff_{}", module_name.to_lowercase())
    }

    pub fn class_ffi_prefix(module_prefix: &str, class_name: &str) -> String {
        format!("{}_{}", module_prefix, class_name.to_lowercase())
    }
}
