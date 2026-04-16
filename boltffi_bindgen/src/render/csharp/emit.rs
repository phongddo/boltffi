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
}
