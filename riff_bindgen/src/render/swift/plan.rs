use crate::ir::codec::CodecPlan;
use crate::render::swift::codec;

#[derive(Debug, Clone)]
pub struct SwiftModule {
    pub imports: Vec<String>,
    pub records: Vec<SwiftRecord>,
    pub enums: Vec<SwiftEnum>,
    pub classes: Vec<SwiftClass>,
    pub callbacks: Vec<SwiftCallback>,
    pub functions: Vec<SwiftFunction>,
}

#[derive(Debug, Clone)]
pub struct SwiftRecord {
    pub class_name: String,
    pub fields: Vec<SwiftField>,
    pub is_blittable: bool,
}

#[derive(Debug, Clone)]
pub struct SwiftField {
    pub swift_name: String,
    pub swift_type: String,
    pub default_expr: Option<String>,
    pub codec: CodecPlan,
}

impl SwiftField {
    pub fn wire_decode_inline(&self) -> String {
        codec::decode_inline(&self.codec)
    }

    pub fn wire_size_expr(&self) -> String {
        codec::size_expr(&self.codec, &self.swift_name)
    }

    pub fn wire_encode(&self) -> String {
        codec::encode_data(&self.codec, &self.swift_name)
    }

    pub fn wire_encode_bytes(&self) -> String {
        codec::encode_bytes(&self.codec, &self.swift_name)
    }
}

#[derive(Debug, Clone)]
pub struct SwiftEnum {
    pub name: String,
    pub variants: Vec<SwiftVariant>,
    pub is_c_style: bool,
    pub is_error: bool,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SwiftVariant {
    pub swift_name: String,
    pub discriminant: i64,
    pub payload: SwiftVariantPayload,
}

impl SwiftVariant {
    pub fn is_unit(&self) -> bool {
        matches!(self.payload, SwiftVariantPayload::Unit)
    }

    pub fn is_single_tuple(&self) -> bool {
        match &self.payload {
            SwiftVariantPayload::Struct(fields) => {
                fields.len() == 1 && fields[0].swift_name.chars().all(|c| c.is_ascii_digit())
            }
            _ => false,
        }
    }

    pub fn fields(&self) -> &[SwiftField] {
        match &self.payload {
            SwiftVariantPayload::Unit => &[],
            SwiftVariantPayload::Tuple(fields) | SwiftVariantPayload::Struct(fields) => fields,
        }
    }

    fn single_tuple_field(&self) -> Option<&SwiftField> {
        if let SwiftVariantPayload::Struct(fields) = &self.payload
            && fields.len() == 1
            && fields[0].swift_name.chars().all(|c| c.is_ascii_digit())
        {
            return Some(&fields[0]);
        }
        None
    }

    pub fn tuple_value_decode(&self) -> String {
        self.single_tuple_field()
            .map(|f| codec::decode_inline(&f.codec))
            .unwrap_or_default()
    }

    pub fn tuple_value_size(&self) -> String {
        self.single_tuple_field()
            .map(|f| codec::size_expr(&f.codec, "value"))
            .unwrap_or_default()
    }

    pub fn tuple_value_encode(&self) -> String {
        self.single_tuple_field()
            .map(|f| codec::encode_data(&f.codec, "value"))
            .unwrap_or_default()
    }

    pub fn tuple_value_encode_bytes(&self) -> String {
        self.single_tuple_field()
            .map(|f| codec::encode_bytes(&f.codec, "value"))
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub enum SwiftVariantPayload {
    Unit,
    Tuple(Vec<SwiftField>),
    Struct(Vec<SwiftField>),
}

#[derive(Debug, Clone)]
pub struct SwiftClass {
    pub name: String,
    pub ffi_free: String,
    pub constructors: Vec<SwiftConstructor>,
    pub methods: Vec<SwiftMethod>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SwiftConstructor {
    pub name: Option<String>,
    pub ffi_symbol: String,
    pub params: Vec<SwiftParam>,
    pub is_fallible: bool,
    pub doc: Option<String>,
}

impl SwiftConstructor {
    pub fn params_signature(&self) -> String {
        self.params
            .iter()
            .map(|p| p.signature())
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn ffi_args(&self) -> String {
        self.params
            .iter()
            .map(|p| p.ffi_arg())
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn has_wrappers(&self) -> bool {
        self.params.iter().any(|p| p.needs_wrapper())
    }
}

#[derive(Debug, Clone)]
pub struct SwiftMethod {
    pub name: String,
    pub ffi_symbol: String,
    pub params: Vec<SwiftParam>,
    pub returns: SwiftReturn,
    pub is_static: bool,
    pub is_async: bool,
    pub doc: Option<String>,
}

impl SwiftMethod {
    pub fn signature(&self) -> String {
        let params_str: String = self
            .params
            .iter()
            .map(|p| p.signature())
            .collect::<Vec<_>>()
            .join(", ");

        let mut sig = format!(
            "public{} func {}({})",
            if self.is_static { " static" } else { "" },
            self.name,
            params_str
        );

        if self.is_async {
            sig.push_str(" async");
        }
        if self.returns.is_throws() {
            sig.push_str(" throws");
        }
        if let Some(ret_type) = self.returns.swift_type() {
            sig.push_str(&format!(" -> {}", ret_type));
        }

        sig
    }

    pub fn ffi_args_with_handle(&self) -> String {
        std::iter::once("handle".to_string())
            .chain(self.params.iter().map(|p| p.ffi_arg()))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn ffi_args(&self) -> String {
        self.params
            .iter()
            .map(|p| p.ffi_arg())
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn needs_handle(&self) -> bool {
        !self.is_static
    }
}

#[derive(Debug, Clone)]
pub struct SwiftCallback {
    pub protocol_name: String,
    pub wrapper_class: String,
    pub vtable_var: String,
    pub vtable_type: String,
    pub bridge_name: String,
    pub register_fn: String,
    pub create_fn: String,
    pub methods: Vec<SwiftCallbackMethod>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SwiftCallbackMethod {
    pub swift_name: String,
    pub ffi_name: String,
    pub params: Vec<SwiftCallbackParam>,
    pub returns: SwiftReturn,
    pub is_async: bool,
    pub has_out_param: bool,
}

#[derive(Debug, Clone)]
pub struct SwiftCallbackParam {
    pub label: String,
    pub swift_type: String,
    pub call_arg: String,
    pub ffi_args: Vec<String>,
    pub decode_prelude: Option<String>,
}

impl SwiftCallbackMethod {
    pub fn has_return(&self) -> bool {
        !self.returns.is_void()
    }

    pub fn throws(&self) -> bool {
        self.returns.is_throws()
    }

    pub fn return_type(&self) -> Option<String> {
        self.returns.swift_type()
    }

    pub fn wire_encoded_return(&self) -> bool {
        self.returns.is_wire_encoded()
    }

    pub fn wire_return_encode(&self) -> Option<String> {
        self.encoded_return_codec().map(|codec| {
            let size_expr = codec::size_expr(codec, "result");
            let encode_expr = codec::encode_data(codec, "result");
            format!(
                "let encoded = ({{ var data = Data(capacity: {}); {}; return data }})()",
                size_expr, encode_expr
            )
        })
    }

    fn encoded_return_codec(&self) -> Option<&CodecPlan> {
        match &self.returns {
            SwiftReturn::FromWireBuffer { codec, .. } => Some(codec),
            SwiftReturn::Throws { ok, .. } => match ok.as_ref() {
                SwiftReturn::FromWireBuffer { codec, .. } => Some(codec),
                _ => None,
            },
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SwiftFunction {
    pub name: String,
    pub ffi_symbol: String,
    pub params: Vec<SwiftParam>,
    pub returns: SwiftReturn,
    pub is_async: bool,
    pub doc: Option<String>,
}

impl SwiftFunction {
    pub fn signature(&self) -> String {
        let params_str: String = self
            .params
            .iter()
            .map(|p| p.signature())
            .collect::<Vec<_>>()
            .join(", ");

        let mut sig = format!("public func {}({})", self.name, params_str);

        if self.is_async {
            sig.push_str(" async");
        }
        if self.returns.is_throws() {
            sig.push_str(" throws");
        }
        if let Some(ret_type) = self.returns.swift_type() {
            sig.push_str(&format!(" -> {}", ret_type));
        }

        sig
    }

    pub fn ffi_args(&self) -> String {
        self.params
            .iter()
            .map(|p| p.ffi_arg())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Debug, Clone)]
pub struct SwiftParam {
    pub label: Option<String>,
    pub name: String,
    pub swift_type: String,
    pub conversion: SwiftConversion,
}

impl SwiftParam {
    pub fn signature(&self) -> String {
        match &self.label {
            Some(label) if label != &self.name => {
                format!("{} {}: {}", label, self.name, self.swift_type)
            }
            _ => format!("{}: {}", self.name, self.swift_type),
        }
    }

    pub fn ffi_arg(&self) -> String {
        match &self.conversion {
            SwiftConversion::Direct => self.name.clone(),
            SwiftConversion::ToString => format!("{}.cString", self.name),
            SwiftConversion::ToData => format!(
                "{}.withUnsafeBytes {{ $0.baseAddress }}, UInt32({}.count)",
                self.name, self.name
            ),
            SwiftConversion::ToWireBuffer { .. } => {
                format!("{}_buf.ptr, UInt32({}_buf.len)", self.name, self.name)
            }
            SwiftConversion::WrapCallback { .. } => format!("{}_ptr, {}_fn", self.name, self.name),
            SwiftConversion::PassHandle { nullable, .. } => {
                if *nullable {
                    format!("{}?.handle", self.name)
                } else {
                    format!("{}.handle", self.name)
                }
            }
        }
    }

    pub fn needs_wrapper(&self) -> bool {
        !matches!(self.conversion, SwiftConversion::Direct)
    }

    pub fn wrapper_code(&self) -> Option<String> {
        match &self.conversion {
            SwiftConversion::ToWireBuffer { .. } => Some(format!(
                "let {}_buf = {}.wireEncoded()\n    defer {{ {}_buf.deallocate() }}",
                self.name, self.name, self.name
            )),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SwiftConversion {
    Direct,
    ToString,
    ToData,
    ToWireBuffer { codec: CodecPlan },
    WrapCallback { protocol: String },
    PassHandle { class_name: String, nullable: bool },
}

#[derive(Debug, Clone)]
pub enum SwiftReturn {
    Void,
    Direct {
        swift_type: String,
    },
    FromWireBuffer {
        swift_type: String,
        codec: CodecPlan,
    },
    Handle {
        class_name: String,
        nullable: bool,
    },
    Throws {
        ok: Box<SwiftReturn>,
        err_type: String,
    },
}

impl SwiftReturn {
    pub fn swift_type(&self) -> Option<String> {
        match self {
            SwiftReturn::Void => None,
            SwiftReturn::Direct { swift_type } => Some(swift_type.clone()),
            SwiftReturn::FromWireBuffer { swift_type, .. } => Some(swift_type.clone()),
            SwiftReturn::Handle {
                class_name,
                nullable,
            } => {
                if *nullable {
                    Some(format!("{}?", class_name))
                } else {
                    Some(class_name.clone())
                }
            }
            SwiftReturn::Throws { ok, .. } => ok.swift_type(),
        }
    }

    pub fn is_throws(&self) -> bool {
        matches!(self, SwiftReturn::Throws { .. })
    }

    pub fn is_void(&self) -> bool {
        matches!(self, SwiftReturn::Void)
    }

    pub fn is_wire_encoded(&self) -> bool {
        match self {
            SwiftReturn::FromWireBuffer { .. } => true,
            SwiftReturn::Throws { ok, .. } => ok.is_wire_encoded(),
            _ => false,
        }
    }

    pub fn is_handle(&self) -> bool {
        matches!(self, SwiftReturn::Handle { .. })
    }

    pub fn handle_info(&self) -> Option<(&str, bool)> {
        match self {
            SwiftReturn::Handle {
                class_name,
                nullable,
            } => Some((class_name.as_str(), *nullable)),
            _ => None,
        }
    }
}
