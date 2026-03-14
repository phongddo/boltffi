use super::JavaVersion;

#[derive(Debug, Clone)]
pub struct JavaModule {
    pub package_name: String,
    pub class_name: String,
    pub lib_name: String,
    pub java_version: JavaVersion,
    pub prefix: String,
    pub records: Vec<JavaRecord>,
    pub enums: Vec<JavaEnum>,
    pub functions: Vec<JavaFunction>,
}

impl JavaModule {
    pub fn package_path(&self) -> String {
        self.package_name.replace('.', "/")
    }

    pub fn has_wire_params(&self) -> bool {
        self.functions.iter().any(|f| !f.wire_writers.is_empty())
    }

    pub fn needs_wire_writer(&self) -> bool {
        self.has_wire_params() || !self.records.is_empty() || self.has_data_enums()
    }

    pub fn has_data_enums(&self) -> bool {
        self.enums.iter().any(|e| !e.is_c_style())
    }
}

#[derive(Debug, Clone)]
pub struct JavaEnum {
    pub class_name: String,
    pub kind: JavaEnumKind,
    pub value_type: String,
    pub variants: Vec<JavaEnumVariant>,
}

impl JavaEnum {
    pub fn tag_literal(&self, tag: &i128) -> String {
        match self.value_type.as_str() {
            "byte" => format!("(byte) {}", tag),
            "short" => format!("(short) {}", tag),
            "long" => format!("{}L", tag),
            _ => tag.to_string(),
        }
    }

    pub fn is_c_style(&self) -> bool {
        matches!(self.kind, JavaEnumKind::CStyle)
    }

    pub fn is_sealed(&self) -> bool {
        matches!(self.kind, JavaEnumKind::SealedInterface)
    }

    pub fn is_abstract(&self) -> bool {
        matches!(self.kind, JavaEnumKind::AbstractClass)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaEnumKind {
    CStyle,
    SealedInterface,
    AbstractClass,
}

#[derive(Debug, Clone)]
pub struct JavaEnumVariant {
    pub name: String,
    pub tag: i128,
    pub fields: Vec<JavaEnumField>,
}

impl JavaEnumVariant {
    pub fn is_unit(&self) -> bool {
        self.fields.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct JavaEnumField {
    pub name: String,
    pub java_type: String,
    pub wire_decode_expr: String,
    pub wire_size_expr: String,
    pub wire_encode_expr: String,
}

#[derive(Debug, Clone)]
pub struct JavaRecord {
    pub shape: JavaRecordShape,
    pub class_name: String,
    pub fields: Vec<JavaRecordField>,
}

impl JavaRecord {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn uses_native_record_syntax(&self) -> bool {
        matches!(self.shape, JavaRecordShape::NativeRecord)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaRecordShape {
    ClassicClass,
    NativeRecord,
}

#[derive(Debug, Clone)]
pub struct JavaRecordField {
    pub name: String,
    pub java_type: String,
    pub wire_decode_expr: String,
    pub wire_size_expr: String,
    pub wire_encode_expr: String,
    pub equals_expr: String,
    pub hash_expr: String,
}

#[derive(Debug, Clone)]
pub enum JavaReturnStrategy {
    Void,
    Direct,
    CStyleEnumDecode {
        class_name: String,
        native_type: String,
    },
    WireDecode {
        decode_expr: String,
    },
}

impl JavaReturnStrategy {
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Direct)
    }

    pub fn is_c_style_enum(&self) -> bool {
        matches!(self, Self::CStyleEnumDecode { .. })
    }

    pub fn is_wire(&self) -> bool {
        matches!(self, Self::WireDecode { .. })
    }

    pub fn decode_expr(&self) -> &str {
        match self {
            Self::WireDecode { decode_expr } => decode_expr,
            _ => "",
        }
    }

    pub fn c_style_enum_class(&self) -> &str {
        match self {
            Self::CStyleEnumDecode { class_name, .. } => class_name,
            _ => "",
        }
    }

    pub fn native_return_type<'a>(&'a self, return_type: &'a str) -> &'a str {
        match self {
            Self::Void => "void",
            Self::Direct => return_type,
            Self::CStyleEnumDecode { native_type, .. } => native_type,
            Self::WireDecode { .. } => "byte[]",
        }
    }
}

#[derive(Debug, Clone)]
pub struct JavaFunction {
    pub name: String,
    pub ffi_name: String,
    pub params: Vec<JavaParam>,
    pub return_type: String,
    pub strategy: JavaReturnStrategy,
    pub wire_writers: Vec<JavaWireWriter>,
}

impl JavaFunction {
    pub fn native_return_type(&self) -> &str {
        self.strategy.native_return_type(&self.return_type)
    }
}

#[derive(Debug, Clone)]
pub struct JavaWireWriter {
    pub binding_name: String,
    pub param_name: String,
    pub size_expr: String,
    pub encode_expr: String,
}

#[derive(Debug, Clone)]
pub struct JavaParam {
    pub name: String,
    pub java_type: String,
    pub native_type: String,
    pub native_expr: String,
}
