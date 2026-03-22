use super::JavaVersion;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaAsyncMode {
    CompletableFuture,
    VirtualThread,
}

impl JavaAsyncMode {
    pub fn from_version(version: JavaVersion) -> Self {
        if version.supports_virtual_threads() {
            Self::VirtualThread
        } else {
            Self::CompletableFuture
        }
    }

    pub fn is_completable_future(&self) -> bool {
        matches!(self, Self::CompletableFuture)
    }

    pub fn is_virtual_thread(&self) -> bool {
        matches!(self, Self::VirtualThread)
    }
}

#[derive(Debug, Clone)]
pub struct JavaAsyncCall {
    pub poll: String,
    pub complete: String,
    pub cancel: String,
    pub free: String,
    pub complete_return_plan: JavaReturnPlan,
}

#[derive(Debug, Clone)]
pub struct JavaModule {
    pub package_name: String,
    pub class_name: String,
    pub lib_name: String,
    pub java_version: JavaVersion,
    pub async_mode: JavaAsyncMode,
    pub prefix: String,
    pub records: Vec<JavaRecord>,
    pub enums: Vec<JavaEnum>,
    pub closures: Vec<JavaClosureInterface>,
    pub callbacks: Vec<JavaCallbackTrait>,
    pub functions: Vec<JavaFunction>,
    pub classes: Vec<JavaClass>,
}

impl JavaModule {
    pub fn package_path(&self) -> String {
        self.package_name.replace('.', "/")
    }

    pub fn has_async(&self) -> bool {
        self.functions.iter().any(|f| f.async_call.is_some())
            || self
                .classes
                .iter()
                .any(|c| c.methods.iter().any(|m| m.async_call.is_some()))
    }

    pub fn has_wire_params(&self) -> bool {
        self.functions.iter().any(|f| !f.wire_writers.is_empty())
            || self.classes.iter().any(|c| c.has_wire_params())
    }

    pub fn needs_wire_writer(&self) -> bool {
        self.has_wire_params() || !self.records.is_empty() || self.has_data_enums()
    }

    pub fn has_data_enums(&self) -> bool {
        self.enums.iter().any(|e| !e.is_c_style() && !e.is_error())
    }

    pub fn has_closures(&self) -> bool {
        !self.closures.is_empty()
    }

    pub fn has_callbacks(&self) -> bool {
        !self.callbacks.is_empty()
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

    pub fn is_error(&self) -> bool {
        matches!(self.kind, JavaEnumKind::Error)
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
    Error,
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
    pub equals_expr: String,
    pub hash_expr: String,
}

#[derive(Debug, Clone)]
pub struct JavaRecord {
    pub shape: JavaRecordShape,
    pub class_name: String,
    pub fields: Vec<JavaRecordField>,
    pub blittable_layout: Option<JavaBlittableLayout>,
}

impl JavaRecord {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn is_blittable(&self) -> bool {
        self.blittable_layout.is_some()
    }

    pub fn uses_native_record_syntax(&self) -> bool {
        matches!(self.shape, JavaRecordShape::NativeRecord)
    }
}

#[derive(Debug, Clone)]
pub struct JavaBlittableLayout {
    pub struct_size: usize,
    pub fields: Vec<JavaBlittableField>,
}

#[derive(Debug, Clone)]
pub struct JavaBlittableField {
    pub name: String,
    pub const_name: String,
    pub offset: usize,
    pub decode_expr: String,
    pub encode_expr: String,
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
pub struct JavaReturnPlan {
    pub native_return_type: String,
    pub render: JavaReturnRender,
}

#[derive(Debug, Clone)]
pub enum JavaReturnRender {
    Void,
    Direct,
    CStyleEnum {
        class_name: String,
    },
    Decode {
        decode_expr: String,
    },
    Handle {
        class_name: String,
        nullable: bool,
    },
    Result {
        ok_decode_expr: String,
        err_decode_expr: String,
        err_is_string: bool,
        err_exception_class: Option<String>,
    },
}

impl JavaReturnPlan {
    pub fn is_void(&self) -> bool {
        matches!(self.render, JavaReturnRender::Void)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self.render, JavaReturnRender::Direct)
    }

    pub fn is_c_style_enum(&self) -> bool {
        matches!(self.render, JavaReturnRender::CStyleEnum { .. })
    }

    pub fn is_decode(&self) -> bool {
        matches!(self.render, JavaReturnRender::Decode { .. })
    }

    pub fn is_handle(&self) -> bool {
        matches!(self.render, JavaReturnRender::Handle { .. })
    }

    pub fn is_result(&self) -> bool {
        matches!(self.render, JavaReturnRender::Result { .. })
    }

    pub fn result_ok_decode(&self) -> &str {
        match &self.render {
            JavaReturnRender::Result { ok_decode_expr, .. } => ok_decode_expr,
            _ => "",
        }
    }

    pub fn result_err_decode(&self) -> &str {
        match &self.render {
            JavaReturnRender::Result {
                err_decode_expr, ..
            } => err_decode_expr,
            _ => "",
        }
    }

    pub fn result_err_is_string(&self) -> bool {
        matches!(
            self.render,
            JavaReturnRender::Result {
                err_is_string: true,
                ..
            }
        )
    }

    pub fn result_err_exception_class(&self) -> &str {
        match &self.render {
            JavaReturnRender::Result {
                err_exception_class: Some(class),
                ..
            } => class,
            _ => "",
        }
    }

    pub fn result_has_typed_exception(&self) -> bool {
        matches!(
            self.render,
            JavaReturnRender::Result {
                err_exception_class: Some(_),
                ..
            }
        )
    }

    pub fn decode_expr(&self) -> &str {
        match &self.render {
            JavaReturnRender::Decode { decode_expr } => decode_expr,
            _ => "",
        }
    }

    pub fn c_style_enum_class(&self) -> &str {
        match &self.render {
            JavaReturnRender::CStyleEnum { class_name } => class_name,
            _ => "",
        }
    }

    pub fn handle_class(&self) -> &str {
        match &self.render {
            JavaReturnRender::Handle { class_name, .. } => class_name,
            _ => "",
        }
    }

    pub fn handle_nullable(&self) -> bool {
        matches!(self.render, JavaReturnRender::Handle { nullable: true, .. })
    }
}

#[derive(Debug, Clone)]
pub struct JavaFunction {
    pub name: String,
    pub ffi_name: String,
    pub params: Vec<JavaParam>,
    pub return_type: String,
    pub return_plan: JavaReturnPlan,
    pub wire_writers: Vec<JavaWireWriter>,
    pub async_call: Option<JavaAsyncCall>,
}

impl JavaFunction {
    pub fn is_async(&self) -> bool {
        self.async_call.is_some()
    }

    pub fn boxed_return_type(&self) -> &str {
        box_java_type(&self.return_type)
    }

    pub fn native_return_type(&self) -> &str {
        &self.return_plan.native_return_type
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

#[derive(Debug, Clone)]
pub struct JavaClass {
    pub class_name: String,
    pub ffi_free: String,
    pub constructors: Vec<JavaConstructor>,
    pub methods: Vec<JavaClassMethod>,
}

impl JavaClass {
    pub fn has_factory_constructors(&self) -> bool {
        self.constructors
            .iter()
            .any(|c| matches!(c.kind, JavaConstructorKind::Factory))
    }

    pub fn has_static_methods(&self) -> bool {
        self.methods.iter().any(|m| m.is_static)
    }

    pub fn has_async_methods(&self) -> bool {
        self.methods.iter().any(|m| m.async_call.is_some())
    }

    pub fn has_wire_params(&self) -> bool {
        self.constructors.iter().any(|c| !c.wire_writers.is_empty())
            || self.methods.iter().any(|m| !m.wire_writers.is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaConstructorKind {
    Primary,
    Factory,
    Secondary,
}

#[derive(Debug, Clone)]
pub struct JavaConstructor {
    pub kind: JavaConstructorKind,
    pub name: String,
    pub is_fallible: bool,
    pub params: Vec<JavaParam>,
    pub ffi_name: String,
    pub wire_writers: Vec<JavaWireWriter>,
}

impl JavaConstructor {
    pub fn is_factory(&self) -> bool {
        matches!(self.kind, JavaConstructorKind::Factory)
    }
}

#[derive(Debug, Clone)]
pub struct JavaClassMethod {
    pub name: String,
    pub ffi_name: String,
    pub is_static: bool,
    pub params: Vec<JavaParam>,
    pub return_type: String,
    pub return_plan: JavaReturnPlan,
    pub wire_writers: Vec<JavaWireWriter>,
    pub async_call: Option<JavaAsyncCall>,
}

impl JavaClassMethod {
    pub fn is_async(&self) -> bool {
        self.async_call.is_some()
    }

    pub fn boxed_return_type(&self) -> &str {
        box_java_type(&self.return_type)
    }

    pub fn native_return_type(&self) -> &str {
        &self.return_plan.native_return_type
    }
}

#[derive(Debug, Clone)]
pub struct JavaClosureInterface {
    pub interface_name: String,
    pub callback_id: String,
    pub params: Vec<JavaClosureParam>,
    pub return_type: Option<String>,
    pub jni_return_type: Option<String>,
    pub return_to_jni_expr: String,
}

impl JavaClosureInterface {
    pub fn is_void_return(&self) -> bool {
        self.return_type.is_none()
    }

    pub fn boxed_return_type(&self) -> &str {
        self.return_type
            .as_deref()
            .map(box_java_type)
            .unwrap_or("Void")
    }
}

#[derive(Debug, Clone)]
pub struct JavaClosureParam {
    pub name: String,
    pub java_type: String,
    pub jni_type: String,
    pub jni_decode_expr: String,
}

#[derive(Debug, Clone)]
pub struct JavaCallbackTrait {
    pub interface_name: String,
    pub callback_id: String,
    pub methods: Vec<JavaCallbackMethod>,
}

#[derive(Debug, Clone)]
pub struct JavaCallbackMethod {
    pub name: String,
    pub ffi_name: String,
    pub params: Vec<JavaCallbackParam>,
    pub return_info: Option<JavaCallbackReturn>,
}

#[derive(Debug, Clone)]
pub struct JavaCallbackParam {
    pub name: String,
    pub java_type: String,
    pub jni_type: String,
    pub decode_expr: String,
}

#[derive(Debug, Clone)]
pub struct JavaCallbackReturn {
    pub java_type: String,
    pub jni_type: String,
    pub default_value: String,
    pub to_jni_expr: String,
    pub wrap_prefix: String,
}

impl JavaCallbackReturn {
    pub fn has_wrap(&self) -> bool {
        !self.wrap_prefix.is_empty()
    }
}

fn box_java_type(java_type: &str) -> &str {
    match java_type {
        "void" => "Void",
        "boolean" => "Boolean",
        "byte" => "Byte",
        "short" => "Short",
        "int" => "Integer",
        "long" => "Long",
        "float" => "Float",
        "double" => "Double",
        other => other,
    }
}
