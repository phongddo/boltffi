use askama::Template;
use std::collections::HashMap;

use crate::model::{BuiltinId, Module, Primitive, Record, Type};

use super::super::names::NamingConvention;
use super::super::types::TypeMapper;
use super::super::wire;

#[derive(Template)]
#[template(path = "swift/record.txt", escape = "none")]
pub struct RecordTemplate {
    pub class_name: String,
    pub fields: Vec<FieldView>,
    pub is_blittable: bool,
}

impl RecordTemplate {
    pub fn from_record(record: &Record, module: &Module) -> Self {
        let mut default_expr_resolver = DefaultExprResolver::new(module);
        let fields: Vec<FieldView> = record
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                Self::make_field(field, module, &mut default_expr_resolver, index)
            })
            .collect();
        let is_blittable = record
            .fields
            .iter()
            .all(|f| Self::is_type_blittable(&f.field_type));
        Self {
            class_name: NamingConvention::class_name(&record.name),
            fields,
            is_blittable,
        }
    }

    fn is_type_blittable(ty: &Type) -> bool {
        matches!(ty, Type::Primitive(_))
    }

    fn make_field(
        field: &crate::model::RecordField,
        module: &Module,
        default_expr_resolver: &mut DefaultExprResolver<'_>,
        _index: usize,
    ) -> FieldView {
        let swift_name = NamingConvention::property_name(&field.name);
        let encoder = wire::encode_type(&field.field_type, &swift_name, module);

        FieldView {
            swift_name: swift_name.clone(),
            swift_type: TypeMapper::map_type(&field.field_type),
            default_expr: default_expr_resolver.default_expr(&field.field_type),
            wire_size_expr: encoder.size_expr,
            wire_decode_inline: Self::make_decode_inline(&field.field_type, module),
            wire_encode: encoder.encode_to_data,
            wire_encode_bytes: encoder.encode_to_bytes,
        }
    }

    fn make_decode_inline(ty: &Type, module: &Module) -> String {
        let codec = wire::decode_type(ty, module);
        let reader = codec.reader_expr.replace("OFFSET", "pos");
        match &codec.size_kind {
            wire::SizeKind::Fixed(size) => {
                format!("{{ let v = {}; pos += {}; return v }}()", reader, size)
            }
            wire::SizeKind::Variable => {
                format!("{{ let (v, s) = {}; pos += s; return v }}()", reader)
            }
        }
    }
}

pub struct FieldView {
    pub swift_name: String,
    pub swift_type: String,
    pub default_expr: Option<String>,
    pub wire_size_expr: String,
    pub wire_decode_inline: String,
    pub wire_encode: String,
    pub wire_encode_bytes: String,
}

#[derive(Clone)]
enum DefaultExprState {
    Visiting,
    Known(Option<String>),
}

struct DefaultExprResolver<'a> {
    module: &'a Module,
    records: HashMap<String, DefaultExprState>,
    enums: HashMap<String, DefaultExprState>,
}

impl<'a> DefaultExprResolver<'a> {
    fn new(module: &'a Module) -> Self {
        Self {
            module,
            records: HashMap::new(),
            enums: HashMap::new(),
        }
    }

    fn default_expr(&mut self, ty: &Type) -> Option<String> {
        match ty {
            Type::Primitive(primitive) => Some(primitive.default_value().to_string()),
            Type::String => Some("\"\"".to_string()),
            Type::Bytes => Some("Data()".to_string()),
            Type::Builtin(builtin) => builtin_default_expr(*builtin),
            Type::Vec(inner) if matches!(inner.as_ref(), Type::Primitive(Primitive::U8)) => {
                Some("Data()".to_string())
            }
            Type::Vec(_) => Some("[]".to_string()),
            Type::Option(_) => Some("nil".to_string()),
            Type::Record(name) => self.default_record_expr(name),
            Type::Enum(name) => self.default_enum_expr(name),
            Type::Custom { name: _, repr } => self.default_expr(repr),
            Type::Void
            | Type::Slice(_)
            | Type::MutSlice(_)
            | Type::Result { .. }
            | Type::Closure(_)
            | Type::Object(_)
            | Type::BoxedTrait(_) => None,
        }
    }

    fn default_record_expr(&mut self, record_name: &str) -> Option<String> {
        match self.records.get(record_name).cloned() {
            Some(DefaultExprState::Known(value)) => return value,
            Some(DefaultExprState::Visiting) => return None,
            None => {}
        }

        self.records
            .insert(record_name.to_string(), DefaultExprState::Visiting);

        let default_expr = self
            .module
            .find_record(record_name)
            .map(|record| {
                record
                    .fields
                    .iter()
                    .map(|field| self.default_expr(&field.field_type))
                    .all(|value| value.is_some())
            })
            .unwrap_or(false)
            .then(|| NamingConvention::class_name(record_name))
            .map(|name| format!("{}()", name));

        self.records.insert(
            record_name.to_string(),
            DefaultExprState::Known(default_expr.clone()),
        );

        default_expr
    }

    fn default_enum_expr(&mut self, enum_name: &str) -> Option<String> {
        match self.enums.get(enum_name).cloned() {
            Some(DefaultExprState::Known(value)) => return value,
            Some(DefaultExprState::Visiting) => return None,
            None => {}
        }

        self.enums
            .insert(enum_name.to_string(), DefaultExprState::Visiting);

        let default_expr = self.module.find_enum(enum_name).and_then(|enumeration| {
            if !enumeration.is_data_enum() {
                return enumeration
                    .variants
                    .first()
                    .map(|variant| NamingConvention::enum_case_name(&variant.name))
                    .map(|case_name| format!(".{}", case_name));
            }

            enumeration.variants.iter().find_map(|variant| {
                if variant.fields.is_empty() {
                    return Some(format!(
                        ".{}",
                        NamingConvention::enum_case_name(&variant.name)
                    ));
                }

                let is_single_tuple =
                    variant.fields.len() == 1 && variant.fields[0].name.starts_with('_');
                let field_defaults: Vec<Option<String>> = variant
                    .fields
                    .iter()
                    .map(|field| self.default_expr(&field.field_type))
                    .collect();

                if field_defaults.iter().any(|value| value.is_none()) {
                    return None;
                }

                let case_name = NamingConvention::enum_case_name(&variant.name);
                let defaults = field_defaults.into_iter().flatten().collect::<Vec<_>>();

                if is_single_tuple {
                    return defaults
                        .first()
                        .cloned()
                        .map(|default_value| format!(".{}({})", case_name, default_value));
                }

                let labeled_defaults = variant
                    .fields
                    .iter()
                    .zip(defaults.iter())
                    .map(|(field, default_value)| {
                        let swift_name = NamingConvention::param_name(&field.name);
                        format!("{}: {}", swift_name, default_value)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");

                Some(format!(".{}({})", case_name, labeled_defaults))
            })
        });

        self.enums.insert(
            enum_name.to_string(),
            DefaultExprState::Known(default_expr.clone()),
        );

        default_expr
    }
}

fn builtin_default_expr(builtin: BuiltinId) -> Option<String> {
    match builtin {
        BuiltinId::Duration => Some("0".to_string()),
        BuiltinId::SystemTime => Some("Date(timeIntervalSince1970: 0)".to_string()),
        BuiltinId::Uuid => {
            Some("UUID(uuidString: \"00000000-0000-0000-0000-000000000000\")!".to_string())
        }
        BuiltinId::Url => Some("URL(string: \"about:blank\")!".to_string()),
    }
}
