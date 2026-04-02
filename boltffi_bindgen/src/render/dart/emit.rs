use askama::Template as _;

use crate::{
    ir::{AbiContract, FfiContract, PrimitiveType},
    render::dart::{
        lower::DartLowerer,
        templates::{NativeFunctionsTemplate, PreludeTemplate},
    },
};

pub struct DartEmitter {}

impl DartEmitter {
    pub fn emit(ffi: &FfiContract, abi: &AbiContract, package_name: &str) -> String {
        let lowerer = DartLowerer::new(ffi, abi, package_name);

        let library = lowerer.library();

        let mut output = String::new();

        output.push_str(PreludeTemplate {}.render().unwrap().as_str());
        output.push_str("\n\n");

        output.push_str(
            NativeFunctionsTemplate {
                cfuncs: &library.native.functions,
            }
            .render()
            .unwrap()
            .as_str(),
        );
        output.push_str("\n\n");

        output
    }
}

pub fn primitive_dart_type(primitive: PrimitiveType) -> String {
    match primitive {
        PrimitiveType::Bool => "bool".to_string(),
        PrimitiveType::I8
        | PrimitiveType::U8
        | PrimitiveType::I16
        | PrimitiveType::U16
        | PrimitiveType::I32
        | PrimitiveType::U32
        | PrimitiveType::I64
        | PrimitiveType::U64
        | PrimitiveType::ISize
        | PrimitiveType::USize => "int".to_string(),
        PrimitiveType::F32 | PrimitiveType::F64 => "double".to_string(),
    }
}

pub fn primitive_native_type(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "$$ffi.Bool",
        PrimitiveType::I8 => "$$ffi.Int8",
        PrimitiveType::I16 => "$$ffi.Int16",
        PrimitiveType::I32 => "$$ffi.Int32",
        PrimitiveType::I64 => "$$ffi.Int64",
        PrimitiveType::U8 => "$$ffi.Uint8",
        PrimitiveType::U16 => "$$ffi.Uint16",
        PrimitiveType::U32 => "$$ffi.Uint32",
        PrimitiveType::U64 => "$$ffi.Uint64",
        PrimitiveType::ISize => "$$ffi.IntPtr",
        PrimitiveType::USize => "$$ffi.UintPtr",
        PrimitiveType::F32 => "$$ffi.Float",
        PrimitiveType::F64 => "$$ffi.Double",
    }
}
