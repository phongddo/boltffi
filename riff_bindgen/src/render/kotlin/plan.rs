use crate::ir::codec::VecLayout;
use crate::ir::ids::CallbackId;

#[derive(Clone)]
pub struct KotlinModule {
    pub package_name: String,
    pub prefix: String,
    pub extra_imports: Vec<String>,
    pub custom_types: Vec<KotlinCustomType>,
    pub enums: Vec<KotlinEnum>,
    pub data_enum_codecs: Vec<KotlinDataEnumCodec>,
    pub records: Vec<KotlinRecord>,
    pub record_readers: Vec<KotlinRecordReader>,
    pub record_writers: Vec<KotlinRecordWriter>,
    pub closures: Vec<KotlinClosureInterface>,
    pub functions: Vec<KotlinFunction>,
    pub classes: Vec<KotlinClass>,
    pub callbacks: Vec<KotlinCallbackTrait>,
    pub native: KotlinNative,
    pub api_style: KotlinApiStyle,
    pub module_object_name: Option<String>,
}

#[derive(Clone, Copy)]
pub enum KotlinApiStyle {
    TopLevel,
    ModuleObject,
}

#[derive(Clone)]
pub struct KotlinCustomType {
    pub class_name: String,
    pub repr_kotlin_type: String,
    pub repr_size_expr: String,
    pub repr_encode_expr: String,
    pub repr_decode_pair_expr: String,
}

#[derive(Clone)]
pub struct KotlinEnum {
    pub class_name: String,
    pub variants: Vec<KotlinEnumVariant>,
    pub is_c_style: bool,
    pub is_error: bool,
}

#[derive(Clone)]
pub struct KotlinEnumVariant {
    pub name: String,
    pub tag: i64,
    pub fields: Vec<KotlinEnumField>,
}

#[derive(Clone)]
pub struct KotlinEnumField {
    pub name: String,
    pub kotlin_type: String,
    pub local_name: String,
    pub wire_decode_inline: String,
    pub wire_size_expr: String,
    pub wire_encode: String,
}

#[derive(Clone)]
pub struct KotlinDataEnumCodec {
    pub class_name: String,
    pub codec_name: String,
    pub struct_size: usize,
    pub payload_offset: usize,
    pub variants: Vec<KotlinDataEnumVariant>,
}

#[derive(Clone)]
pub struct KotlinDataEnumVariant {
    pub name: String,
    pub const_name: String,
    pub tag_value: i64,
    pub fields: Vec<KotlinDataEnumField>,
}

#[derive(Clone)]
pub struct KotlinDataEnumField {
    pub param_name: String,
    pub value_expr: String,
    pub offset: usize,
    pub getter: String,
    pub putter: String,
    pub conversion: String,
}

#[derive(Clone)]
pub struct KotlinRecord {
    pub class_name: String,
    pub fields: Vec<KotlinRecordField>,
    pub is_blittable: bool,
    pub struct_size: usize,
}

#[derive(Clone)]
pub struct KotlinRecordField {
    pub name: String,
    pub kotlin_type: String,
    pub has_default: bool,
    pub default_expr: String,
    pub read_expr: String,
    pub local_name: String,
    pub wire_decode_inline: String,
    pub wire_size_expr: String,
    pub wire_encode: String,
    pub padding_after: usize,
}

#[derive(Clone)]
pub struct KotlinRecordReader {
    pub reader_name: String,
    pub class_name: String,
    pub struct_size: usize,
    pub fields: Vec<KotlinRecordReaderField>,
}

#[derive(Clone)]
pub struct KotlinRecordReaderField {
    pub name: String,
    pub const_name: String,
    pub offset: usize,
    pub getter: String,
    pub conversion: String,
}

#[derive(Clone)]
pub struct KotlinRecordWriter {
    pub writer_name: String,
    pub class_name: String,
    pub struct_size: usize,
    pub fields: Vec<KotlinRecordWriterField>,
}

#[derive(Clone)]
pub struct KotlinRecordWriterField {
    pub const_name: String,
    pub offset: usize,
    pub putter: String,
    pub value_expr: String,
}

#[derive(Clone)]
pub struct KotlinClosureInterface {
    pub interface_name: String,
    pub params: Vec<KotlinSignatureParam>,
    pub return_type: String,
    pub is_void_return: bool,
}

#[derive(Clone)]
pub struct KotlinFunction {
    pub func_name: String,
    pub signature_params: Vec<KotlinSignatureParam>,
    pub return_type: Option<String>,
    pub wire_writers: Vec<KotlinWireWriter>,
    pub wire_writer_closes: Vec<String>,
    pub native_args: Vec<String>,
    pub throws: bool,
    pub err_type: String,
    pub ffi_name: String,
    pub return_abi: KotlinReturnAbi,
    pub is_async: bool,
    pub async_call: Option<KotlinAsyncCall>,
    pub decode_expr: String,
    pub is_blittable_return: bool,
}

#[derive(Clone)]
pub struct KotlinClass {
    pub class_name: String,
    pub doc: Option<String>,
    pub prefix: String,
    pub ffi_free: String,
    pub constructors: Vec<KotlinConstructor>,
    pub methods: Vec<KotlinMethod>,
    pub use_companion_methods: bool,
    pub has_factory_ctors: bool,
}

#[derive(Clone)]
pub struct KotlinConstructor {
    pub name: String,
    pub is_factory: bool,
    pub is_fallible: bool,
    pub signature_params: Vec<KotlinSignatureParam>,
    pub wire_writers: Vec<KotlinWireWriter>,
    pub wire_writer_closes: Vec<String>,
    pub native_args: Vec<String>,
    pub ffi_name: String,
}

#[derive(Clone)]
pub struct KotlinMethod {
    pub impl_: KotlinMethodImpl,
}

#[derive(Clone)]
pub enum KotlinMethodImpl {
    AsyncMethod(String),
    SyncMethod(String),
}

#[derive(Clone)]
pub struct KotlinCallbackTrait {
    pub interface_name: String,
    pub handle_map_name: String,
    pub callbacks_object: String,
    pub bridge_name: String,
    pub doc: Option<String>,
    pub sync_methods: Vec<KotlinCallbackMethod>,
    pub async_methods: Vec<KotlinAsyncCallbackMethod>,
}

#[derive(Clone)]
pub struct KotlinCallbackMethod {
    pub name: String,
    pub ffi_name: String,
    pub params: Vec<KotlinCallbackParam>,
    pub return_info: Option<KotlinCallbackReturn>,
}

#[derive(Clone)]
pub struct KotlinAsyncCallbackMethod {
    pub name: String,
    pub ffi_name: String,
    pub invoker_name: String,
    pub params: Vec<KotlinCallbackParam>,
    pub return_info: Option<KotlinCallbackReturn>,
}

#[derive(Clone)]
pub struct KotlinCallbackParam {
    pub name: String,
    pub kotlin_type: String,
    pub jni_type: String,
    pub conversion: String,
}

#[derive(Clone)]
pub struct KotlinCallbackReturn {
    pub kotlin_type: String,
    pub jni_type: String,
    pub default_value: String,
    pub to_jni: String,
}

#[derive(Clone)]
pub struct KotlinNative {
    pub lib_name: String,
    pub prefix: String,
    pub functions: Vec<KotlinNativeFunction>,
    pub wire_functions: Vec<KotlinNativeWireFunction>,
    pub classes: Vec<KotlinNativeClass>,
    pub async_callback_invokers: Vec<KotlinAsyncCallbackInvoker>,
}

#[derive(Clone)]
pub struct KotlinNativeFunction {
    pub ffi_name: String,
    pub params: Vec<KotlinNativeParam>,
    pub return_jni_type: String,
    pub is_async: bool,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_cancel: String,
    pub ffi_free: String,
    pub complete_return_jni_type: String,
}

#[derive(Clone)]
pub struct KotlinNativeWireFunction {
    pub ffi_name: String,
    pub params: Vec<KotlinNativeParam>,
    pub return_jni_type: String,
}

#[derive(Clone)]
pub struct KotlinNativeClass {
    pub ffi_free: String,
    pub ctors: Vec<KotlinNativeCtor>,
    pub async_methods: Vec<KotlinNativeAsyncMethod>,
    pub sync_methods: Vec<KotlinNativeSyncMethod>,
}

#[derive(Clone)]
pub struct KotlinNativeCtor {
    pub ffi_name: String,
    pub params: Vec<KotlinNativeParam>,
}

#[derive(Clone)]
pub struct KotlinNativeAsyncMethod {
    pub ffi_name: String,
    pub ffi_poll: String,
    pub ffi_complete: String,
    pub ffi_cancel: String,
    pub ffi_free: String,
    pub include_handle: bool,
    pub params: Vec<KotlinNativeParam>,
    pub return_jni_type: String,
}

#[derive(Clone)]
pub struct KotlinNativeSyncMethod {
    pub ffi_name: String,
    pub include_handle: bool,
    pub params: Vec<KotlinNativeParam>,
    pub return_jni_type: String,
}

#[derive(Clone)]
pub struct KotlinNativeParam {
    pub name: String,
    pub jni_type: String,
}

#[derive(Clone)]
pub struct KotlinAsyncCallbackInvoker {
    pub name: String,
    pub jni_type: String,
    pub has_result: bool,
}

#[derive(Clone)]
pub struct KotlinSignatureParam {
    pub name: String,
    pub kotlin_type: String,
}

#[derive(Clone)]
pub struct KotlinWireWriter {
    pub binding_name: String,
    pub size_expr: String,
    pub encode_expr: String,
}

#[derive(Clone)]
pub struct KotlinAsyncCall {
    pub poll: String,
    pub complete: String,
    pub cancel: String,
    pub free: String,
    pub return_abi: KotlinReturnAbi,
}

#[derive(Clone)]
pub enum KotlinReturnAbi {
    Unit,
    Direct { kotlin_cast: String },
    WireEncoded,
}

impl KotlinReturnAbi {
    pub fn is_unit(&self) -> bool {
        matches!(self, Self::Unit)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Direct { .. })
    }

    pub fn is_wire_encoded(&self) -> bool {
        matches!(self, Self::WireEncoded)
    }

    pub fn kotlin_cast(&self) -> &str {
        match self {
            Self::Direct { kotlin_cast } => kotlin_cast,
            _ => "",
        }
    }
}

#[derive(Clone)]
pub struct KotlinVecLayout {
    pub layout: VecLayout,
    pub element_type: String,
}

#[derive(Clone)]
pub struct KotlinCallbackHandle {
    pub callback_id: CallbackId,
    pub class_name: String,
}
