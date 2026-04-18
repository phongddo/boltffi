use crate::ir::definitions::{EnumDef, EnumRepr};
use crate::ir::types::PrimitiveType;

use super::names::NamingConvention;
use super::plan::CSharpType;

/// Maps a BoltFFI primitive to the corresponding [`CSharpType`].
///
/// C# has native unsigned types (`byte`, `ushort`, `uint`, `ulong`) and
/// platform-sized integers (`nint`, `nuint`), so every primitive maps
/// to a distinct C# type with no information loss.
pub fn csharp_type(primitive: PrimitiveType) -> CSharpType {
    match primitive {
        PrimitiveType::Bool => CSharpType::Bool,
        PrimitiveType::I8 => CSharpType::SByte,
        PrimitiveType::U8 => CSharpType::Byte,
        PrimitiveType::I16 => CSharpType::Short,
        PrimitiveType::U16 => CSharpType::UShort,
        PrimitiveType::I32 => CSharpType::Int,
        PrimitiveType::U32 => CSharpType::UInt,
        PrimitiveType::I64 => CSharpType::Long,
        PrimitiveType::U64 => CSharpType::ULong,
        PrimitiveType::ISize => CSharpType::NInt,
        PrimitiveType::USize => CSharpType::NUInt,
        PrimitiveType::F32 => CSharpType::Float,
        PrimitiveType::F64 => CSharpType::Double,
    }
}

/// Maps a Rust C-style enum repr primitive to a valid C# enum backing type.
///
/// C# enums may only be backed by the fixed-width integral primitives —
/// `nint` / `nuint` are not legal enum base types, so `isize` / `usize`
/// stay unsupported for now.
pub fn csharp_enum_backing_type(tag_type: PrimitiveType) -> Option<CSharpType> {
    match tag_type {
        PrimitiveType::I8 => Some(CSharpType::SByte),
        PrimitiveType::U8 => Some(CSharpType::Byte),
        PrimitiveType::I16 => Some(CSharpType::Short),
        PrimitiveType::U16 => Some(CSharpType::UShort),
        PrimitiveType::I32 => Some(CSharpType::Int),
        PrimitiveType::U32 => Some(CSharpType::UInt),
        PrimitiveType::I64 => Some(CSharpType::Long),
        PrimitiveType::U64 => Some(CSharpType::ULong),
        PrimitiveType::Bool
        | PrimitiveType::ISize
        | PrimitiveType::USize
        | PrimitiveType::F32
        | PrimitiveType::F64 => None,
    }
}

/// Maps a resolved enum definition to the corresponding [`CSharpType`].
///
/// The `EnumRepr` drives the split: a C-style enum (all unit variants)
/// becomes [`CSharpType::CStyleEnum`] and rides P/Invoke as its declared
/// backing integral type; a data enum (at least one payload-carrying
/// variant) becomes
/// [`CSharpType::DataEnum`] and wire-encodes. Everything downstream — the
/// return-kind dispatch, param marshaling, record blittability — keys off
/// this one decision.
pub fn csharp_enum_type(enum_def: &EnumDef) -> CSharpType {
    let class_name = NamingConvention::class_name(enum_def.id.as_str());
    match &enum_def.repr {
        EnumRepr::CStyle { .. } => CSharpType::CStyleEnum(class_name),
        EnumRepr::Data { .. } => CSharpType::DataEnum(class_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::definitions::{CStyleVariant, DataVariant, VariantPayload};
    use crate::ir::ids::EnumId;

    fn enum_def(id: &str, repr: EnumRepr) -> EnumDef {
        EnumDef {
            id: EnumId::new(id),
            repr,
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        }
    }

    #[test]
    fn c_style_repr_maps_to_c_style_enum_type() {
        let def = enum_def(
            "Status",
            EnumRepr::CStyle {
                tag_type: PrimitiveType::I32,
                variants: vec![CStyleVariant {
                    name: "Active".into(),
                    discriminant: 0,
                    doc: None,
                }],
            },
        );
        assert_eq!(
            csharp_enum_type(&def),
            CSharpType::CStyleEnum("Status".to_string())
        );
    }

    #[test]
    fn data_repr_maps_to_data_enum_type() {
        let def = enum_def(
            "Shape",
            EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![DataVariant {
                    name: "Point".into(),
                    discriminant: 0,
                    payload: VariantPayload::Unit,
                    doc: None,
                }],
            },
        );
        assert_eq!(
            csharp_enum_type(&def),
            CSharpType::DataEnum("Shape".to_string())
        );
    }

    /// `class_name` runs the source `snake_case` enum name through
    /// [`NamingConvention::class_name`], so the C# type keeps the name in
    /// PascalCase even if upstream ever shifts the ID casing.
    #[test]
    fn class_name_round_trips_through_naming_convention() {
        let def = enum_def(
            "log_level",
            EnumRepr::CStyle {
                tag_type: PrimitiveType::I32,
                variants: vec![],
            },
        );
        assert_eq!(
            csharp_enum_type(&def),
            CSharpType::CStyleEnum("LogLevel".to_string())
        );
    }

    #[test]
    fn enum_backing_type_maps_u8_to_byte() {
        assert_eq!(
            csharp_enum_backing_type(PrimitiveType::U8),
            Some(CSharpType::Byte)
        );
    }

    #[test]
    fn enum_backing_type_rejects_usize() {
        assert_eq!(csharp_enum_backing_type(PrimitiveType::USize), None);
    }
}
