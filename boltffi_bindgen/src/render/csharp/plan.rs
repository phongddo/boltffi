use std::fmt;

use boltffi_ffi_rules::naming::{LibraryName, Name};

/// Represents a lowered C# module, containing everything the templates need
/// to render a `.cs` file.
#[derive(Debug, Clone)]
pub struct CSharpModule {
    /// C# namespace for the generated file (e.g., `"MyApp"`).
    pub namespace: String,
    /// Top-level class name (e.g., `"MyApp"`).
    pub class_name: String,
    /// Native library name used in `[DllImport("...")]` declarations.
    pub lib_name: Name<LibraryName>,
    /// FFI symbol prefix (e.g., `"boltffi"`).
    pub prefix: String,
    /// Top-level primitive functions. Used by both the public wrapper class
    /// and the `[DllImport]` native declarations — C# P/Invoke passes
    /// primitives directly, so one struct serves both layers.
    pub functions: Vec<CSharpFunction>,
}

impl CSharpModule {
    pub fn has_functions(&self) -> bool {
        !self.functions.is_empty()
    }
}

/// A C# type keyword. Includes `Void` so return types and value types share
/// one enum; params never carry `Void` because the lowerer rejects it before
/// constructing a [`CSharpParam`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CSharpType {
    Void,
    Bool,
    SByte,
    Byte,
    Short,
    UShort,
    Int,
    UInt,
    Long,
    ULong,
    NInt,
    NUInt,
    Float,
    Double,
}

impl CSharpType {
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Void => "void",
            Self::Bool => "bool",
            Self::SByte => "sbyte",
            Self::Byte => "byte",
            Self::Short => "short",
            Self::UShort => "ushort",
            Self::Int => "int",
            Self::UInt => "uint",
            Self::Long => "long",
            Self::ULong => "ulong",
            Self::NInt => "nint",
            Self::NUInt => "nuint",
            Self::Float => "float",
            Self::Double => "double",
        }
    }

    pub fn is_void(self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_bool(self) -> bool {
        matches!(self, Self::Bool)
    }
}

impl fmt::Display for CSharpType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.keyword())
    }
}

/// A primitive function binding. Serves double duty: the template uses `name`
/// and C# types for the public static method, and `ffi_name` for the
/// `[DllImport]` entry point.
#[derive(Debug, Clone)]
pub struct CSharpFunction {
    /// PascalCase method name (e.g., `"EchoI32"`).
    pub name: String,
    /// Parameters with C# types.
    pub params: Vec<CSharpParam>,
    /// C# return type.
    pub return_type: CSharpType,
    /// The C symbol name (e.g., `"boltffi_echo_i32"`).
    pub ffi_name: String,
}

impl CSharpFunction {
    pub fn is_void(&self) -> bool {
        self.return_type.is_void()
    }
}

/// A parameter in a C# function.
#[derive(Debug, Clone)]
pub struct CSharpParam {
    /// camelCase parameter name, keyword-escaped with `@` if needed.
    pub name: String,
    /// C# type.
    pub csharp_type: CSharpType,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn function_with_return(return_type: CSharpType) -> CSharpFunction {
        CSharpFunction {
            name: "Test".to_string(),
            params: vec![],
            return_type,
            ffi_name: "boltffi_test".to_string(),
        }
    }

    #[rstest]
    #[case::void(CSharpType::Void, true)]
    #[case::int(CSharpType::Int, false)]
    #[case::bool(CSharpType::Bool, false)]
    #[case::double(CSharpType::Double, false)]
    fn is_void(#[case] return_type: CSharpType, #[case] expected: bool) {
        assert_eq!(function_with_return(return_type).is_void(), expected);
    }
}
