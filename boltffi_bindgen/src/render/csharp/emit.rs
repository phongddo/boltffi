//! Orchestrates the lowerer and templates to produce the final `.cs` source output.

use askama::Template as _;

use crate::ir::{AbiContract, FfiContract};

use super::{
    CSharpOptions,
    lower::CSharpLowerer,
    templates::{FunctionsTemplate, NativeTemplate, PreambleTemplate},
};

/// The rendered C# output: source code plus metadata for file naming.
#[derive(Debug, Clone)]
pub struct CSharpOutput {
    /// The generated C# source code.
    pub source: String,
    /// The top-level class name (used for the file name, e.g., `"MyApp.cs"`).
    pub class_name: String,
    /// The C# namespace.
    pub namespace: String,
}

/// Entry point for C# code generation. Creates the lowerer, walks the
/// contracts, feeds the plan into templates, and produces a [`CSharpOutput`].
pub struct CSharpEmitter;

impl CSharpEmitter {
    pub fn emit(ffi: &FfiContract, abi: &AbiContract, options: &CSharpOptions) -> CSharpOutput {
        let lowerer = CSharpLowerer::new(ffi, abi, options);
        let module = lowerer.lower();

        let mut source = String::new();

        source.push_str(&PreambleTemplate { module: &module }.render().unwrap());
        source.push('\n');
        source.push_str(&FunctionsTemplate { module: &module }.render().unwrap());
        source.push_str(&NativeTemplate { module: &module }.render().unwrap());
        source.push('\n');

        CSharpOutput {
            class_name: module.class_name,
            namespace: module.namespace,
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Lowerer as IrLowerer;
    use crate::ir::contract::{FfiContract, PackageInfo};
    use crate::ir::definitions::{FunctionDef, ParamDef, ParamPassing, ReturnDef};
    use crate::ir::ids::{FunctionId, ParamName};
    use crate::ir::types::{PrimitiveType, TypeExpr};
    use boltffi_ffi_rules::callable::ExecutionKind;

    fn empty_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "demo_lib".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        }
    }

    fn primitive_function(
        name: &str,
        params: Vec<(&str, PrimitiveType)>,
        returns: ReturnDef,
    ) -> FunctionDef {
        FunctionDef {
            id: FunctionId::new(name),
            params: params
                .into_iter()
                .map(|(param_name, prim)| ParamDef {
                    name: ParamName::new(param_name),
                    type_expr: TypeExpr::Primitive(prim),
                    passing: ParamPassing::Value,
                    doc: None,
                })
                .collect(),
            returns,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        }
    }

    fn emit_contract(contract: &FfiContract) -> CSharpOutput {
        let abi = IrLowerer::new(contract).to_abi_contract();
        CSharpEmitter::emit(contract, &abi, &CSharpOptions::default())
    }

    fn assert_source_contains(source: &str, snippet: &str, expecting: &str) {
        assert!(
            source.contains(snippet),
            "expecting {expecting}\n  missing snippet: {snippet:?}"
        );
    }

    fn assert_source_lacks(source: &str, snippet: &str, expecting: &str) {
        assert!(
            !source.contains(snippet),
            "expecting {expecting}\n  unexpected snippet: {snippet:?}"
        );
    }

    #[test]
    fn emit_primitive_function_generates_wrapper_and_native() {
        let mut contract = empty_contract();
        contract.functions.push(primitive_function(
            "echo_i32",
            vec![("value", PrimitiveType::I32)],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        ));

        let output = emit_contract(&contract);

        assert!(
            output
                .source
                .contains("public static int EchoI32(int value)")
        );
        assert!(
            output
                .source
                .contains("return NativeMethods.EchoI32(value);")
        );
        assert!(
            output
                .source
                .contains(r#"[DllImport(LibName, EntryPoint = "boltffi_echo_i32")]"#)
        );
        assert!(
            output
                .source
                .contains("internal static extern int EchoI32(int value);")
        );
    }

    #[test]
    fn emit_void_function_omits_return_keyword() {
        let mut contract = empty_contract();
        contract
            .functions
            .push(primitive_function("noop", vec![], ReturnDef::Void));

        let output = emit_contract(&contract);

        assert!(output.source.contains("public static void Noop()"));
        assert!(output.source.contains("NativeMethods.Noop();"));
        assert!(!output.source.contains("return NativeMethods.Noop()"));
    }

    #[test]
    fn emit_unsigned_types_use_csharp_unsigned_keywords() {
        let mut contract = empty_contract();
        contract.functions.push(primitive_function(
            "unsigned_echo",
            vec![
                ("a", PrimitiveType::U8),
                ("b", PrimitiveType::U16),
                ("c", PrimitiveType::U32),
                ("d", PrimitiveType::U64),
            ],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::U32)),
        ));

        let output = emit_contract(&contract);

        assert!(
            output
                .source
                .contains("uint UnsignedEcho(byte a, ushort b, uint c, ulong d)")
        );
    }

    #[test]
    fn emit_namespace_and_class_use_pascal_case() {
        let contract = empty_contract();
        let output = emit_contract(&contract);

        assert_eq!(output.namespace, "DemoLib");
        assert_eq!(output.class_name, "DemoLib");
        assert!(output.source.contains("namespace DemoLib"));
    }

    #[test]
    fn emit_escapes_csharp_keywords_in_param_names() {
        let mut contract = empty_contract();
        contract.functions.push(primitive_function(
            "test_keywords",
            vec![("int", PrimitiveType::I32), ("value", PrimitiveType::I32)],
            ReturnDef::Void,
        ));

        let output = emit_contract(&contract);

        assert!(output.source.contains("@int"));
    }

    fn function_with_types(
        name: &str,
        params: Vec<(&str, TypeExpr)>,
        returns: ReturnDef,
    ) -> FunctionDef {
        FunctionDef {
            id: FunctionId::new(name),
            params: params
                .into_iter()
                .map(|(param_name, type_expr)| ParamDef {
                    name: ParamName::new(param_name),
                    type_expr,
                    passing: ParamPassing::Value,
                    doc: None,
                })
                .collect(),
            returns,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        }
    }

    /// C# P/Invoke marshals `bool` as a 4-byte Win32 BOOL by default, but
    /// BoltFFI's C ABI uses a 1-byte native bool, so the generated native
    /// signature must force `UnmanagedType.I1` for both param and return.
    #[test]
    fn emit_bool_function_uses_i1_marshalling_for_native_signature() {
        let mut contract = empty_contract();
        contract.functions.push(primitive_function(
            "flip",
            vec![("value", PrimitiveType::Bool)],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::Bool)),
        ));

        let output = emit_contract(&contract);

        assert!(
            output
                .source
                .contains("public static bool Flip(bool value)")
        );
        assert!(
            output
                .source
                .contains("[return: MarshalAs(UnmanagedType.I1)]")
        );
        assert!(output.source.contains(
            "internal static extern bool Flip([MarshalAs(UnmanagedType.I1)] bool value);"
        ));
    }

    /// The C ABI for a `String` parameter is `(const uint8_t* ptr, uintptr_t len)`,
    /// which on the C# side becomes a `byte[]` + `UIntPtr` pair. The wrapper
    /// exposes a plain `string` and UTF-8 encodes it just before the native call.
    #[test]
    fn emit_string_param_marshals_as_byte_array_and_length() {
        let mut contract = empty_contract();
        contract.functions.push(function_with_types(
            "string_length",
            vec![("v", TypeExpr::String)],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::U32)),
        ));

        let src = emit_contract(&contract).source;

        assert_source_contains(
            &src,
            "public static uint StringLength(string v)",
            "the public wrapper to expose the string param as a plain C# `string`",
        );
        assert_source_contains(
            &src,
            "byte[] _vBytes = Encoding.UTF8.GetBytes(v);",
            "UTF-8 encoding of the string into a managed byte[] before the P/Invoke call",
        );
        assert_source_contains(
            &src,
            "NativeMethods.StringLength(_vBytes, (UIntPtr)_vBytes.Length)",
            "the native call to receive the encoded byte[] and its length as two separate arguments",
        );
        assert_source_contains(
            &src,
            "internal static extern uint StringLength(byte[] v, UIntPtr vLen);",
            "the P/Invoke declaration to split a string param into (byte[], UIntPtr) matching the C ABI",
        );
    }

    /// A `String` return is wire-encoded by Rust into a length-prefixed
    /// `FfiBuf` (i32 length + UTF-8 bytes). The wrapper decodes via
    /// `WireReader.ReadString` and must release the native allocation with
    /// `FreeBuf` even if decoding throws; the generated helper also validates
    /// the wire bounds before reading and decodes only the copied prefix of a
    /// rented array.
    #[test]
    fn emit_string_return_decodes_ffibuf_and_frees() {
        let mut contract = empty_contract();
        contract.functions.push(function_with_types(
            "echo_string",
            vec![("v", TypeExpr::String)],
            ReturnDef::Value(TypeExpr::String),
        ));

        let src = emit_contract(&contract).source;

        assert_source_contains(
            &src,
            "public static string EchoString(string v)",
            "the public wrapper to hide the FfiBuf and expose a normal `string` return",
        );
        assert_source_contains(
            &src,
            "FfiBuf _buf = NativeMethods.EchoString(",
            "the native return captured in an `FfiBuf _buf` local so it can be decoded and freed",
        );
        assert_source_contains(
            &src,
            "return WireReader.ReadString(_buf);",
            "WireReader.ReadString to decode the wire-encoded FfiBuf into a managed string",
        );
        assert_source_contains(
            &src,
            "if (bufLen < 4) throw new InvalidOperationException(\"corrupt wire: buffer too small for length prefix\");",
            "a guard that rejects FfiBuf values too short to hold the 4-byte string length prefix before Marshal.ReadInt32",
        );
        assert_source_contains(
            &src,
            "if ((nuint)len + 4 > bufLen) throw new InvalidOperationException(\"corrupt wire: string length exceeds buffer\");",
            "a guard that rejects declared string payloads whose bytes would run past the end of the FfiBuf",
        );
        assert_source_contains(
            &src,
            "byte[] bytes = ArrayPool<byte>.Shared.Rent(len);",
            "ArrayPool rental to avoid allocating a fresh byte[] on every string decode",
        );
        assert_source_contains(
            &src,
            "return Encoding.UTF8.GetString(bytes, 0, len);",
            "UTF-8 decoding of only the copied prefix from the rented array so pooled tail bytes are ignored",
        );
        assert_source_contains(
            &src,
            "NativeMethods.FreeBuf(_buf);",
            "a FreeBuf call in a finally block so the Rust allocator reclaims the buffer even if decoding throws",
        );
        assert_source_contains(
            &src,
            "internal static extern FfiBuf EchoString(byte[] v, UIntPtr vLen);",
            "the P/Invoke signature to return FfiBuf rather than a bare string",
        );
    }

    /// The `FfiBuf` struct, `WireReader`, and `FreeBuf` DllImport are only
    /// needed when a module actually traffics in strings — primitive-only
    /// output should not carry the extra helpers.
    #[test]
    fn emit_string_helpers_only_appear_when_strings_are_used() {
        let mut primitive_only = empty_contract();
        primitive_only.functions.push(primitive_function(
            "add",
            vec![("a", PrimitiveType::I32), ("b", PrimitiveType::I32)],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        ));
        let primitive_src = emit_contract(&primitive_only).source;

        assert_source_lacks(
            &primitive_src,
            "FfiBuf",
            "no FfiBuf struct or references in primitive-only output",
        );
        assert_source_lacks(
            &primitive_src,
            "WireReader",
            "no WireReader helper class in primitive-only output",
        );
        assert_source_lacks(
            &primitive_src,
            "FreeBuf",
            "no FreeBuf DllImport in primitive-only output",
        );
        assert_source_lacks(
            &primitive_src,
            "using System.Text;",
            "no System.Text using directive when Encoding.UTF8 is never referenced",
        );
        assert_source_lacks(
            &primitive_src,
            "using System.Buffers;",
            "no System.Buffers using directive when ArrayPool is never referenced",
        );

        let mut with_string = empty_contract();
        with_string.functions.push(function_with_types(
            "echo",
            vec![("v", TypeExpr::String)],
            ReturnDef::Value(TypeExpr::String),
        ));
        let string_src = emit_contract(&with_string).source;

        assert_source_contains(
            &string_src,
            "internal struct FfiBuf",
            "the FfiBuf struct when strings are used (mirrors the Rust FfiBuf_u8 layout)",
        );
        assert_source_contains(
            &string_src,
            "internal static class WireReader",
            "the WireReader helper class when strings are used",
        );
        assert_source_contains(
            &string_src,
            r#"[DllImport(LibName, EntryPoint = "boltffi_free_buf")]"#,
            "a DllImport binding to boltffi_free_buf when strings are used",
        );
        assert_source_contains(
            &string_src,
            "using System.Text;",
            "the System.Text using directive so Encoding.UTF8 resolves in the wrapper and WireReader",
        );
        assert_source_contains(
            &string_src,
            "using System.Buffers;",
            "the System.Buffers using directive so ArrayPool resolves in WireReader when strings are used",
        );
    }

    /// Functions that mix string and non-string params must only emit the
    /// UTF-8 prep line for the string args and pass non-string args through
    /// unchanged.
    #[test]
    fn emit_mixed_string_and_primitive_params_only_encodes_strings() {
        let mut contract = empty_contract();
        contract.functions.push(function_with_types(
            "repeat_string",
            vec![
                ("v", TypeExpr::String),
                ("count", TypeExpr::Primitive(PrimitiveType::U32)),
            ],
            ReturnDef::Value(TypeExpr::String),
        ));

        let src = emit_contract(&contract).source;

        assert_source_contains(
            &src,
            "byte[] _vBytes = Encoding.UTF8.GetBytes(v);",
            "UTF-8 encoding only for the string param `v`",
        );
        assert_source_lacks(
            &src,
            "Encoding.UTF8.GetBytes(count)",
            "no UTF-8 encoding for the primitive `count` param",
        );
        assert_source_contains(
            &src,
            "NativeMethods.RepeatString(_vBytes, (UIntPtr)_vBytes.Length, count)",
            "the native call to expand only the string into (bytes, length) and pass the primitive through unchanged",
        );
        assert_source_contains(
            &src,
            "internal static extern FfiBuf RepeatString(byte[] v, UIntPtr vLen, uint count);",
            "the P/Invoke signature to split only the string into byte[]+UIntPtr, keeping the primitive uint direct",
        );
    }
}
