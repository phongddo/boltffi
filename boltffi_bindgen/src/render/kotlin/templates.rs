use askama::Template;

use super::plan::{
    KotlinConstructor, KotlinMethod,
    KotlinMethodImpl::{AsyncMethod, SyncMethod},
    KotlinModule, KotlinStreamMode,
};

pub fn kdoc_block(doc: &Option<String>, indent: &str) -> String {
    match doc {
        Some(text) => {
            let mut result = format!("{indent}/**\n");
            text.lines().for_each(|line| {
                if line.is_empty() {
                    result.push_str(&format!("{indent} *\n"));
                } else {
                    result.push_str(&format!("{indent} * {line}\n"));
                }
            });
            result.push_str(&format!("{indent} */\n"));
            result
        }
        None => String::new(),
    }
}

pub fn kotlin_integer_literal(value: &i128, kotlin_type: &str) -> String {
    match kotlin_type {
        "Byte" => format!("({value}L).toByte()"),
        "Short" => format!("({value}L).toShort()"),
        "Int" => {
            if i32::try_from(*value).is_ok() {
                value.to_string()
            } else {
                format!("({value}L).toInt()")
            }
        }
        "Long" => {
            if i64::try_from(*value).is_ok() {
                format!("{value}L")
            } else {
                format!("({value}uL).toLong()")
            }
        }
        _ => value.to_string(),
    }
}

#[derive(Template)]
#[template(path = "render_kotlin/preamble.txt", escape = "none")]
pub struct PreambleTemplate<'a> {
    pub package_name: &'a str,
    pub prefix: &'a str,
    pub extra_imports: &'a [String],
    pub custom_types: &'a [super::plan::KotlinCustomType],
    pub has_streams: bool,
}

#[derive(Template)]
#[template(path = "render_kotlin/native.txt", escape = "none")]
pub struct NativeTemplate<'a> {
    pub lib_name: &'a str,
    pub prefix: &'a str,
    pub functions: &'a [super::plan::KotlinNativeFunction],
    pub wire_functions: &'a [super::plan::KotlinNativeWireFunction],
    pub classes: &'a [super::plan::KotlinNativeClass],
    pub async_callback_invokers: &'a [super::plan::KotlinAsyncCallbackInvoker],
}

#[derive(Template)]
#[template(path = "render_kotlin/record.txt", escape = "none")]
pub struct RecordTemplate<'a> {
    pub class_name: &'a str,
    pub fields: &'a [super::plan::KotlinRecordField],
    pub is_blittable: bool,
    pub struct_size: usize,
    pub constructors: &'a [KotlinConstructor],
    pub methods: &'a [KotlinMethod],
    pub doc: &'a Option<String>,
}

#[derive(Template)]
#[template(path = "render_kotlin/record_reader.txt", escape = "none")]
pub struct RecordReaderTemplate<'a> {
    pub reader_name: &'a str,
    pub class_name: &'a str,
    pub struct_size: usize,
    pub fields: &'a [super::plan::KotlinRecordReaderField],
}

#[derive(Template)]
#[template(path = "render_kotlin/record_writer.txt", escape = "none")]
pub struct RecordWriterTemplate<'a> {
    pub writer_name: &'a str,
    pub class_name: &'a str,
    pub struct_size: usize,
    pub fields: &'a [super::plan::KotlinRecordWriterField],
}

#[derive(Template)]
#[template(path = "render_kotlin/enum_c_style.txt", escape = "none")]
pub struct CStyleEnumTemplate<'a> {
    pub class_name: &'a str,
    pub variants: &'a [super::plan::KotlinEnumVariant],
    pub value_type: &'a str,
    pub constructors: &'a [KotlinConstructor],
    pub methods: &'a [KotlinMethod],
    pub doc: &'a Option<String>,
}

#[derive(Template)]
#[template(path = "render_kotlin/enum_sealed.txt", escape = "none")]
pub struct SealedEnumTemplate<'a> {
    pub class_name: &'a str,
    pub variants: &'a [super::plan::KotlinEnumVariant],
    pub is_error: bool,
    pub constructors: &'a [KotlinConstructor],
    pub methods: &'a [KotlinMethod],
    pub doc: &'a Option<String>,
}

#[derive(Template)]
#[template(path = "render_kotlin/enum_data_codec.txt", escape = "none")]
pub struct DataEnumCodecTemplate<'a> {
    pub class_name: &'a str,
    pub codec_name: &'a str,
    pub struct_size: usize,
    pub payload_offset: usize,
    pub variants: &'a [super::plan::KotlinDataEnumVariant],
}

#[derive(Template)]
#[template(path = "render_kotlin/function_wire.txt", escape = "none")]
pub struct WireFunctionTemplate<'a> {
    pub func_name: &'a str,
    pub signature_params: &'a [super::plan::KotlinSignatureParam],
    pub return_type: Option<&'a str>,
    pub wire_writers: &'a [super::plan::KotlinWireWriter],
    pub wire_writer_closes: &'a [String],
    pub native_args: &'a [String],
    pub throws: bool,
    pub err_type: &'a str,
    pub ffi_name: &'a str,
    pub return_is_unit: bool,
    pub return_is_direct: bool,
    pub return_cast: &'a str,
    pub decode_expr: &'a str,
    pub is_blittable_return: bool,
    pub doc: &'a Option<String>,
}

#[derive(Template)]
#[template(path = "render_kotlin/function_async.txt", escape = "none")]
pub struct AsyncFunctionTemplate<'a> {
    pub func_name: &'a str,
    pub signature_params: &'a [super::plan::KotlinSignatureParam],
    pub return_type: Option<&'a str>,
    pub wire_writers: &'a [super::plan::KotlinWireWriter],
    pub wire_writer_closes: &'a [String],
    pub native_args: &'a [String],
    pub throws: bool,
    pub err_type: &'a str,
    pub ffi_name: &'a str,
    pub include_handle: bool,
    pub ffi_poll: &'a str,
    pub ffi_complete: &'a str,
    pub ffi_cancel: &'a str,
    pub ffi_free: &'a str,
    pub return_is_unit: bool,
    pub return_is_direct: bool,
    pub return_cast: &'a str,
    pub decode_expr: &'a str,
    pub is_blittable_return: bool,
    pub doc: &'a Option<String>,
}

#[derive(Template)]
#[template(path = "render_kotlin/class.txt", escape = "none")]
pub struct ClassTemplate<'a> {
    pub class_name: &'a str,
    pub doc: &'a Option<String>,
    pub constructors: &'a [KotlinConstructor],
    pub methods: &'a [KotlinMethod],
    pub streams: &'a [super::plan::KotlinStream],
    pub use_companion_methods: bool,
    pub has_factory_ctors: bool,
    pub has_static_methods: bool,
    pub prefix: &'a str,
    pub ffi_free: &'a str,
}

#[derive(Template)]
#[template(path = "render_kotlin/method_wire.txt", escape = "none")]
pub struct WireMethodTemplate<'a> {
    pub method_name: &'a str,
    pub signature_params: &'a [super::plan::KotlinSignatureParam],
    pub return_type: Option<&'a str>,
    pub wire_writers: &'a [super::plan::KotlinWireWriter],
    pub wire_writer_closes: &'a [String],
    pub native_args: &'a [String],
    pub throws: bool,
    pub err_type: &'a str,
    pub ffi_name: &'a str,
    pub return_is_unit: bool,
    pub return_is_direct: bool,
    pub return_cast: &'a str,
    pub decode_expr: &'a str,
    pub is_blittable_return: bool,
    pub include_handle: bool,
    pub doc: &'a Option<String>,
}

#[derive(Template)]
#[template(path = "render_kotlin/method_async.txt", escape = "none")]
pub struct AsyncMethodTemplate<'a> {
    pub method_name: &'a str,
    pub signature_params: &'a [super::plan::KotlinSignatureParam],
    pub return_type: Option<&'a str>,
    pub wire_writers: &'a [super::plan::KotlinWireWriter],
    pub wire_writer_closes: &'a [String],
    pub native_args: &'a [String],
    pub throws: bool,
    pub err_type: &'a str,
    pub ffi_name: &'a str,
    pub include_handle: bool,
    pub ffi_poll: &'a str,
    pub ffi_complete: &'a str,
    pub ffi_cancel: &'a str,
    pub ffi_free: &'a str,
    pub return_is_unit: bool,
    pub return_is_direct: bool,
    pub return_cast: &'a str,
    pub decode_expr: &'a str,
    pub is_blittable_return: bool,
    pub doc: &'a Option<String>,
}

#[derive(Template)]
#[template(path = "render_kotlin/callback_trait.txt", escape = "none")]
pub struct CallbackTraitTemplate<'a> {
    pub interface_name: &'a str,
    pub handle_map_name: &'a str,
    pub callbacks_object: &'a str,
    pub bridge_name: &'a str,
    pub doc: &'a Option<String>,
    pub is_closure: bool,
    pub sync_methods: &'a [super::plan::KotlinCallbackMethod],
    pub async_methods: &'a [super::plan::KotlinAsyncCallbackMethod],
}

#[derive(Template)]
#[template(path = "render_kotlin/closure_interface.txt", escape = "none")]
pub struct ClosureInterfaceTemplate<'a> {
    pub interface_name: &'a str,
    pub params: &'a [super::plan::KotlinSignatureParam],
    pub return_type: &'a str,
    pub is_void_return: bool,
}

pub struct KotlinEmitter;

impl KotlinEmitter {
    pub fn emit(module: &KotlinModule) -> String {
        let preamble = PreambleTemplate {
            package_name: &module.package_name,
            prefix: &module.prefix,
            extra_imports: &module.extra_imports,
            custom_types: &module.custom_types,
            has_streams: module.has_streams,
        }
        .render()
        .unwrap();

        let mut declarations = Vec::new();

        module.enums.iter().for_each(|enumeration| {
            let rendered = if enumeration.is_c_style() && !enumeration.is_error() {
                CStyleEnumTemplate {
                    class_name: &enumeration.class_name,
                    variants: &enumeration.variants,
                    value_type: enumeration.c_style_value_type.as_deref().unwrap_or("Int"),
                    constructors: &enumeration.constructors,
                    methods: &enumeration.methods,
                    doc: &enumeration.doc,
                }
                .render()
                .unwrap()
            } else {
                SealedEnumTemplate {
                    class_name: &enumeration.class_name,
                    variants: &enumeration.variants,
                    is_error: enumeration.is_error(),
                    constructors: &enumeration.constructors,
                    methods: &enumeration.methods,
                    doc: &enumeration.doc,
                }
                .render()
                .unwrap()
            };
            declarations.push(rendered);
        });

        module.data_enum_codecs.iter().for_each(|codec| {
            let rendered = DataEnumCodecTemplate {
                class_name: &codec.class_name,
                codec_name: &codec.codec_name,
                struct_size: codec.struct_size,
                payload_offset: codec.payload_offset,
                variants: &codec.variants,
            }
            .render()
            .unwrap();
            declarations.push(rendered);
        });

        module.records.iter().for_each(|record| {
            let rendered = RecordTemplate {
                class_name: &record.class_name,
                fields: &record.fields,
                is_blittable: record.is_blittable,
                struct_size: record.struct_size,
                constructors: &record.constructors,
                methods: &record.methods,
                doc: &record.doc,
            }
            .render()
            .unwrap();
            declarations.push(rendered);
        });

        module.record_readers.iter().for_each(|reader| {
            let rendered = RecordReaderTemplate {
                reader_name: &reader.reader_name,
                class_name: &reader.class_name,
                struct_size: reader.struct_size,
                fields: &reader.fields,
            }
            .render()
            .unwrap();
            declarations.push(rendered);
        });

        module.record_writers.iter().for_each(|writer| {
            let rendered = RecordWriterTemplate {
                writer_name: &writer.writer_name,
                class_name: &writer.class_name,
                struct_size: writer.struct_size,
                fields: &writer.fields,
            }
            .render()
            .unwrap();
            declarations.push(rendered);
        });

        module.closures.iter().for_each(|closure| {
            let rendered = ClosureInterfaceTemplate {
                interface_name: &closure.interface_name,
                params: &closure.params,
                return_type: closure.return_type(),
                is_void_return: closure.is_void_return(),
            }
            .render()
            .unwrap();
            declarations.push(rendered);
        });

        module.functions.iter().for_each(|function| {
            let rendered = if function.is_async() {
                let async_call = function.async_call.as_ref().unwrap();
                AsyncFunctionTemplate {
                    func_name: &function.func_name,
                    signature_params: &function.signature_params,
                    return_type: function.return_type.as_deref(),
                    wire_writers: &function.wire_writers,
                    wire_writer_closes: &function.wire_writer_closes,
                    native_args: &function.native_args,
                    throws: function.throws,
                    err_type: &function.err_type,
                    ffi_name: &function.ffi_name,
                    include_handle: false,
                    ffi_poll: &async_call.poll,
                    ffi_complete: &async_call.complete,
                    ffi_cancel: &async_call.cancel,
                    ffi_free: &async_call.free,
                    return_is_unit: async_call.return_is_unit,
                    return_is_direct: async_call.return_is_direct,
                    return_cast: &async_call.return_cast,
                    decode_expr: &async_call.decode_expr,
                    is_blittable_return: async_call.is_blittable_return,
                    doc: &function.doc,
                }
                .render()
                .unwrap()
            } else {
                WireFunctionTemplate {
                    func_name: &function.func_name,
                    signature_params: &function.signature_params,
                    return_type: function.return_type.as_deref(),
                    wire_writers: &function.wire_writers,
                    wire_writer_closes: &function.wire_writer_closes,
                    native_args: &function.native_args,
                    throws: function.throws,
                    err_type: &function.err_type,
                    ffi_name: &function.ffi_name,
                    return_is_unit: function.return_is_unit,
                    return_is_direct: function.return_is_direct,
                    return_cast: &function.return_cast,
                    decode_expr: &function.decode_expr,
                    is_blittable_return: function.is_blittable_return,
                    doc: &function.doc,
                }
                .render()
                .unwrap()
            };
            declarations.push(rendered);
        });

        module.classes.iter().for_each(|class| {
            let rendered = ClassTemplate {
                class_name: &class.class_name,
                doc: &class.doc,
                constructors: &class.constructors,
                methods: &class.methods,
                streams: &class.streams,
                use_companion_methods: class.use_companion_methods,
                has_factory_ctors: class.has_factory_ctors(),
                has_static_methods: class.has_static_methods(),
                prefix: &class.prefix,
                ffi_free: &class.ffi_free,
            }
            .render()
            .unwrap();
            declarations.push(rendered);
        });

        module.callbacks.iter().for_each(|callback| {
            let rendered = CallbackTraitTemplate {
                interface_name: &callback.interface_name,
                handle_map_name: &callback.handle_map_name,
                callbacks_object: &callback.callbacks_object,
                bridge_name: &callback.bridge_name,
                doc: &callback.doc,
                is_closure: callback.is_closure,
                sync_methods: &callback.sync_methods,
                async_methods: &callback.async_methods,
            }
            .render()
            .unwrap();
            declarations.push(rendered);
        });

        let native = NativeTemplate {
            lib_name: &module.native.lib_name,
            prefix: &module.native.prefix,
            functions: &module.native.functions,
            wire_functions: &module.native.wire_functions,
            classes: &module.native.classes,
            async_callback_invokers: &module.native.async_callback_invokers,
        }
        .render()
        .unwrap();

        let rendered_declarations = match module.api_style {
            super::plan::KotlinApiStyle::TopLevel => declarations
                .iter()
                .map(|section| section.trim().to_string())
                .filter(|section| !section.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n"),
            super::plan::KotlinApiStyle::ModuleObject => {
                let object_name = module
                    .module_object_name
                    .clone()
                    .unwrap_or_else(|| "BoltFFIModule".to_string());
                format!(
                    "object {} {{\n{}\n}}",
                    object_name,
                    declarations
                        .iter()
                        .map(|section| section.trim().to_string())
                        .filter(|section| !section.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n\n")
                )
            }
        };

        let mut output = [preamble, rendered_declarations, native]
            .into_iter()
            .map(|section| section.trim().to_string())
            .filter(|section| !section.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        output.push('\n');
        output
    }
}

#[cfg(all(test, not(miri)))]
mod tests {
    use askama::Template;

    use super::super::plan::{
        KotlinAsyncCallbackMethod, KotlinCallbackMethod, KotlinCallbackParam, KotlinCallbackReturn,
        KotlinClass, KotlinConstructor, KotlinDataEnumField, KotlinDataEnumVariant,
        KotlinEnumField, KotlinEnumVariant, KotlinMethod, KotlinMethodImpl, KotlinRecordField,
        KotlinSignatureParam, KotlinWireWriter,
    };
    use super::*;

    #[test]
    fn snapshot_record_with_field_docs() {
        let template = RecordTemplate {
            constructors: &[],
            methods: &[],
            class_name: "Location",
            fields: &[
                KotlinRecordField {
                    name: "id".to_string(),
                    kotlin_type: "Long".to_string(),
                    default_value: None,
                    wire_decode_expr: "reader.readI64()".to_string(),
                    wire_size_expr: "8".to_string(),
                    wire_encode: "wire.writeI64(id)".to_string(),
                    padding_after: 0,
                    doc: Some("Unique identifier for this location.".to_string()),
                },
                KotlinRecordField {
                    name: "lat".to_string(),
                    kotlin_type: "Double".to_string(),
                    default_value: None,
                    wire_decode_expr: "reader.readF64()".to_string(),
                    wire_size_expr: "8".to_string(),
                    wire_encode: "wire.writeF64(lat)".to_string(),
                    padding_after: 0,
                    doc: Some("Latitude in decimal degrees.".to_string()),
                },
            ],
            is_blittable: true,
            struct_size: 16,
            doc: &Some("A physical location with coordinates.".to_string()),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_record_with_optional_field() {
        let template = RecordTemplate {
            constructors: &[],
            methods: &[],
            class_name: "UserProfile",
            fields: &[
                KotlinRecordField {
                    name: "name".to_string(),
                    kotlin_type: "String".to_string(),
                    default_value: None,
                    wire_decode_expr: "reader.readString()".to_string(),
                    wire_size_expr: "reader.sizeString(name)".to_string(),
                    wire_encode: "wire.writeString(name)".to_string(),
                    padding_after: 0,
                    doc: None,
                },
                KotlinRecordField {
                    name: "bio".to_string(),
                    kotlin_type: "String?".to_string(),
                    default_value: Some("null".to_string()),
                    wire_decode_expr: "reader.readOption { it.readString() }".to_string(),
                    wire_size_expr: "reader.sizeOption(bio) { it.sizeString(it) }".to_string(),
                    wire_encode: "wire.writeOption(bio) { w, v -> w.writeString(v) }".to_string(),
                    padding_after: 0,
                    doc: None,
                },
            ],
            is_blittable: false,
            struct_size: 0,
            doc: &None,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_record_with_default_value() {
        let template = RecordTemplate {
            constructors: &[],
            methods: &[],
            class_name: "Config",
            fields: &[
                KotlinRecordField {
                    name: "timeout".to_string(),
                    kotlin_type: "Int".to_string(),
                    default_value: Some("30".to_string()),
                    wire_decode_expr: "reader.readI32()".to_string(),
                    wire_size_expr: "4".to_string(),
                    wire_encode: "wire.writeI32(timeout)".to_string(),
                    padding_after: 0,
                    doc: None,
                },
                KotlinRecordField {
                    name: "retries".to_string(),
                    kotlin_type: "Int".to_string(),
                    default_value: Some("3".to_string()),
                    wire_decode_expr: "reader.readI32()".to_string(),
                    wire_size_expr: "4".to_string(),
                    wire_encode: "wire.writeI32(retries)".to_string(),
                    padding_after: 0,
                    doc: None,
                },
            ],
            is_blittable: true,
            struct_size: 8,
            doc: &None,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_enum_with_variant_docs() {
        let template = CStyleEnumTemplate {
            constructors: &[],
            methods: &[],
            class_name: "Direction",
            variants: &[
                KotlinEnumVariant {
                    name: "North".to_string(),
                    tag: 0,
                    fields: vec![],
                    doc: Some("Pointing toward the north pole.".to_string()),
                },
                KotlinEnumVariant {
                    name: "South".to_string(),
                    tag: 1,
                    fields: vec![],
                    doc: None,
                },
            ],
            value_type: "Int",
            doc: &Some("A cardinal compass direction.".to_string()),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_enum_with_byte_tag_type() {
        let template = CStyleEnumTemplate {
            constructors: &[],
            methods: &[],
            class_name: "PacketKind",
            variants: &[
                KotlinEnumVariant {
                    name: "Ping".to_string(),
                    tag: 0,
                    fields: vec![],
                    doc: None,
                },
                KotlinEnumVariant {
                    name: "Pong".to_string(),
                    tag: 255,
                    fields: vec![],
                    doc: None,
                },
            ],
            value_type: "Byte",
            doc: &None,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_sealed_enum_with_payloads() {
        let template = SealedEnumTemplate {
            constructors: &[],
            methods: &[],
            class_name: "Result",
            variants: &[
                KotlinEnumVariant {
                    name: "Success".to_string(),
                    tag: 0,
                    fields: vec![KotlinEnumField {
                        name: "value".to_string(),
                        kotlin_type: "String".to_string(),
                        wire_decode_expr: "reader.readString()".to_string(),
                        wire_size_expr: "reader.sizeString(value)".to_string(),
                        wire_encode: "wire.writeString(value)".to_string(),
                    }],
                    doc: Some("Operation succeeded.".to_string()),
                },
                KotlinEnumVariant {
                    name: "Error".to_string(),
                    tag: 1,
                    fields: vec![
                        KotlinEnumField {
                            name: "code".to_string(),
                            kotlin_type: "Int".to_string(),
                            wire_decode_expr: "reader.readI32()".to_string(),
                            wire_size_expr: "4".to_string(),
                            wire_encode: "wire.writeI32(code)".to_string(),
                        },
                        KotlinEnumField {
                            name: "message".to_string(),
                            kotlin_type: "String".to_string(),
                            wire_decode_expr: "reader.readString()".to_string(),
                            wire_size_expr: "reader.sizeString(message)".to_string(),
                            wire_encode: "wire.writeString(message)".to_string(),
                        },
                    ],
                    doc: Some("Operation failed.".to_string()),
                },
            ],
            is_error: false,
            doc: &Some("The result of an operation.".to_string()),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_error_enum() {
        let template = SealedEnumTemplate {
            constructors: &[],
            methods: &[],
            class_name: "ApiError",
            variants: &[
                KotlinEnumVariant {
                    name: "NetworkError".to_string(),
                    tag: 0,
                    fields: vec![KotlinEnumField {
                        name: "message".to_string(),
                        kotlin_type: "String".to_string(),
                        wire_decode_expr: "reader.readString()".to_string(),
                        wire_size_expr: "reader.sizeString(message)".to_string(),
                        wire_encode: "wire.writeString(message)".to_string(),
                    }],
                    doc: None,
                },
                KotlinEnumVariant {
                    name: "NotFound".to_string(),
                    tag: 1,
                    fields: vec![],
                    doc: None,
                },
            ],
            is_error: true,
            doc: &None,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_data_enum_codec() {
        let template = DataEnumCodecTemplate {
            class_name: "Message",
            codec_name: "MessageCodec",
            struct_size: 24,
            payload_offset: 8,
            variants: &[
                KotlinDataEnumVariant {
                    name: "Text".to_string(),
                    const_name: "TAG_TEXT".to_string(),
                    tag_value: 0,
                    fields: vec![KotlinDataEnumField {
                        param_name: "content".to_string(),
                        value_expr: "value.content".to_string(),
                        offset: 8,
                        getter: "getI64".to_string(),
                        putter: "putI64".to_string(),
                        conversion: "".to_string(),
                    }],
                },
                KotlinDataEnumVariant {
                    name: "Image".to_string(),
                    const_name: "TAG_IMAGE".to_string(),
                    tag_value: 1,
                    fields: vec![
                        KotlinDataEnumField {
                            param_name: "width".to_string(),
                            value_expr: "value.width".to_string(),
                            offset: 8,
                            getter: "getI32".to_string(),
                            putter: "putI32".to_string(),
                            conversion: "".to_string(),
                        },
                        KotlinDataEnumField {
                            param_name: "height".to_string(),
                            value_expr: "value.height".to_string(),
                            offset: 12,
                            getter: "getI32".to_string(),
                            putter: "putI32".to_string(),
                            conversion: "".to_string(),
                        },
                    ],
                },
            ],
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_sync_function_returning_primitive() {
        let template = WireFunctionTemplate {
            func_name: "add",
            signature_params: &[
                KotlinSignatureParam {
                    name: "a".to_string(),
                    kotlin_type: "Int".to_string(),
                },
                KotlinSignatureParam {
                    name: "b".to_string(),
                    kotlin_type: "Int".to_string(),
                },
            ],
            return_type: Some("Int"),
            wire_writers: &[],
            wire_writer_closes: &[],
            native_args: &["a".to_string(), "b".to_string()],
            throws: false,
            err_type: "",
            ffi_name: "boltffi_add",
            return_is_unit: false,
            return_is_direct: true,
            return_cast: "",
            decode_expr: "",
            is_blittable_return: false,
            doc: &None,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_sync_function_with_string_param() {
        let template = WireFunctionTemplate {
            func_name: "greet",
            signature_params: &[KotlinSignatureParam {
                name: "name".to_string(),
                kotlin_type: "String".to_string(),
            }],
            return_type: Some("String"),
            wire_writers: &[KotlinWireWriter {
                binding_name: "nameWire".to_string(),
                size_expr: "BoltFFIWire.sizeString(name)".to_string(),
                encode_expr: "BoltFFIWire.writeString(name)".to_string(),
            }],
            wire_writer_closes: &["nameWire.close()".to_string()],
            native_args: &["nameWire.ptr".to_string(), "nameWire.len".to_string()],
            throws: false,
            err_type: "",
            ffi_name: "boltffi_greet",
            return_is_unit: false,
            return_is_direct: false,
            return_cast: "",
            decode_expr: "reader.readString()",
            is_blittable_return: false,
            doc: &None,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_async_function_returning_string() {
        let template = AsyncFunctionTemplate {
            func_name: "fetchData",
            signature_params: &[KotlinSignatureParam {
                name: "url".to_string(),
                kotlin_type: "String".to_string(),
            }],
            return_type: Some("String"),
            wire_writers: &[KotlinWireWriter {
                binding_name: "urlWire".to_string(),
                size_expr: "BoltFFIWire.sizeString(url)".to_string(),
                encode_expr: "BoltFFIWire.writeString(url)".to_string(),
            }],
            wire_writer_closes: &["urlWire.close()".to_string()],
            native_args: &["urlWire.ptr".to_string(), "urlWire.len".to_string()],
            throws: false,
            err_type: "",
            ffi_name: "boltffi_fetch_data",
            include_handle: false,
            ffi_poll: "boltffi_fetch_data_poll",
            ffi_complete: "boltffi_fetch_data_complete",
            ffi_cancel: "boltffi_fetch_data_cancel",
            ffi_free: "boltffi_fetch_data_free",
            return_is_unit: false,
            return_is_direct: false,
            return_cast: "",
            decode_expr: "reader.readString()",
            is_blittable_return: false,
            doc: &None,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_class_with_documented_constructors_and_method() {
        let cls = KotlinClass {
            class_name: "DataStore".to_string(),
            doc: Some("A persistent key-value data store.".to_string()),
            prefix: "boltffi".to_string(),
            ffi_free: "boltffi_data_store_free".to_string(),
            constructors: vec![
                KotlinConstructor {
                    name: "DataStore".to_string(),
                    is_factory: false,
                    is_fallible: false,
                    signature_params: vec![KotlinSignatureParam {
                        name: "capacity".to_string(),
                        kotlin_type: "Int".to_string(),
                    }],
                    wire_writers: vec![],
                    wire_writer_closes: vec![],
                    native_args: vec!["capacity".to_string()],
                    ffi_name: "boltffi_data_store_new".to_string(),
                    doc: Some("Creates a new data store with the given capacity.".to_string()),
                },
                KotlinConstructor {
                    name: "withDefaults".to_string(),
                    is_factory: true,
                    is_fallible: false,
                    signature_params: vec![],
                    wire_writers: vec![],
                    wire_writer_closes: vec![],
                    native_args: vec![],
                    ffi_name: "boltffi_data_store_with_defaults".to_string(),
                    doc: Some("Creates a data store with sensible default settings.".to_string()),
                },
            ],
            methods: vec![KotlinMethod {
                impl_: KotlinMethodImpl::SyncMethod(
                    "/**\n * Inserts a value into the store by key.\n */\nfun insert(key: String) { Native.boltffi_data_store_insert(handle, key) }".to_string(),
                ),
                is_static: false,
            }],
            streams: vec![],
            use_companion_methods: true,
        };
        let template = ClassTemplate {
            class_name: &cls.class_name,
            doc: &cls.doc,
            constructors: &cls.constructors,
            methods: &cls.methods,
            streams: &cls.streams,
            use_companion_methods: cls.use_companion_methods,
            has_factory_ctors: cls.has_factory_ctors(),
            has_static_methods: cls.has_static_methods(),
            prefix: &cls.prefix,
            ffi_free: &cls.ffi_free,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_class_with_fallible_constructor() {
        let cls = KotlinClass {
            class_name: "Connection".to_string(),
            doc: None,
            prefix: "boltffi".to_string(),
            ffi_free: "boltffi_connection_free".to_string(),
            constructors: vec![KotlinConstructor {
                name: "Connection".to_string(),
                is_factory: false,
                is_fallible: true,
                signature_params: vec![KotlinSignatureParam {
                    name: "url".to_string(),
                    kotlin_type: "String".to_string(),
                }],
                wire_writers: vec![KotlinWireWriter {
                    binding_name: "urlWire".to_string(),
                    size_expr: "BoltFFIWire.sizeString(url)".to_string(),
                    encode_expr: "BoltFFIWire.writeString(url)".to_string(),
                }],
                wire_writer_closes: vec!["urlWire.close()".to_string()],
                native_args: vec!["urlWire.ptr".to_string(), "urlWire.len".to_string()],
                ffi_name: "boltffi_connection_open".to_string(),
                doc: None,
            }],
            methods: vec![],
            streams: vec![],
            use_companion_methods: false,
        };
        let template = ClassTemplate {
            class_name: &cls.class_name,
            doc: &cls.doc,
            constructors: &cls.constructors,
            methods: &cls.methods,
            streams: &cls.streams,
            use_companion_methods: cls.use_companion_methods,
            has_factory_ctors: cls.has_factory_ctors(),
            has_static_methods: cls.has_static_methods(),
            prefix: &cls.prefix,
            ffi_free: &cls.ffi_free,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_class_with_static_method() {
        let cls = KotlinClass {
            class_name: "Logger".to_string(),
            doc: None,
            prefix: "boltffi".to_string(),
            ffi_free: "boltffi_logger_free".to_string(),
            constructors: vec![],
            methods: vec![KotlinMethod {
                impl_: KotlinMethodImpl::SyncMethod(
                    "fun getDefault(): Logger = Logger(Native.boltffi_logger_get_default())"
                        .to_string(),
                ),
                is_static: true,
            }],
            streams: vec![],
            use_companion_methods: true,
        };
        let template = ClassTemplate {
            class_name: &cls.class_name,
            doc: &cls.doc,
            constructors: &cls.constructors,
            methods: &cls.methods,
            streams: &cls.streams,
            use_companion_methods: cls.use_companion_methods,
            has_factory_ctors: cls.has_factory_ctors(),
            has_static_methods: cls.has_static_methods(),
            prefix: &cls.prefix,
            ffi_free: &cls.ffi_free,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_class_with_async_method() {
        let cls = KotlinClass {
            class_name: "HttpClient".to_string(),
            doc: None,
            prefix: "boltffi".to_string(),
            ffi_free: "boltffi_http_client_free".to_string(),
            constructors: vec![],
            methods: vec![KotlinMethod {
                impl_: KotlinMethodImpl::AsyncMethod(
                    "suspend fun fetch(url: String): ByteArray { /* async impl */ }".to_string(),
                ),
                is_static: false,
            }],
            streams: vec![],
            use_companion_methods: false,
        };
        let template = ClassTemplate {
            class_name: &cls.class_name,
            doc: &cls.doc,
            constructors: &cls.constructors,
            methods: &cls.methods,
            streams: &cls.streams,
            use_companion_methods: cls.use_companion_methods,
            has_factory_ctors: cls.has_factory_ctors(),
            has_static_methods: cls.has_static_methods(),
            prefix: &cls.prefix,
            ffi_free: &cls.ffi_free,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_callback_trait_simple() {
        let template = CallbackTraitTemplate {
            interface_name: "DataHandler",
            handle_map_name: "DataHandlerMap",
            callbacks_object: "DataHandlerCallbacks",
            bridge_name: "DataHandlerBridge",
            doc: &None,
            is_closure: false,
            sync_methods: &[KotlinCallbackMethod {
                name: "onData".to_string(),
                ffi_name: "on_data".to_string(),
                params: vec![KotlinCallbackParam {
                    name: "data".to_string(),
                    kotlin_type: "ByteArray".to_string(),
                    jni_type: "ByteArray".to_string(),
                    conversion: "data".to_string(),
                }],
                return_info: None,
                doc: None,
            }],
            async_methods: &[],
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_callback_trait_with_return() {
        let template = CallbackTraitTemplate {
            interface_name: "Validator",
            handle_map_name: "ValidatorMap",
            callbacks_object: "ValidatorCallbacks",
            bridge_name: "ValidatorBridge",
            doc: &Some("Validates input strings.".to_string()),
            is_closure: false,
            sync_methods: &[KotlinCallbackMethod {
                name: "validate".to_string(),
                ffi_name: "validate".to_string(),
                params: vec![KotlinCallbackParam {
                    name: "input".to_string(),
                    kotlin_type: "String".to_string(),
                    jni_type: "String".to_string(),
                    conversion: "input".to_string(),
                }],
                return_info: Some(KotlinCallbackReturn {
                    kotlin_type: "Boolean".to_string(),
                    jni_type: "Boolean".to_string(),
                    default_value: "false".to_string(),
                    to_jni: "".to_string(),
                    to_jni_result: None,
                    error_type: None,
                    error_is_throwable: false,
                }),
                doc: None,
            }],
            async_methods: &[],
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_callback_with_async_method() {
        let template = CallbackTraitTemplate {
            interface_name: "AsyncHandler",
            handle_map_name: "AsyncHandlerMap",
            callbacks_object: "AsyncHandlerCallbacks",
            bridge_name: "AsyncHandlerBridge",
            doc: &None,
            is_closure: false,
            sync_methods: &[],
            async_methods: &[KotlinAsyncCallbackMethod {
                name: "onComplete".to_string(),
                ffi_name: "on_complete".to_string(),
                invoker_name: "invokeOnComplete".to_string(),
                params: vec![KotlinCallbackParam {
                    name: "result".to_string(),
                    kotlin_type: "String".to_string(),
                    jni_type: "String".to_string(),
                    conversion: "result".to_string(),
                }],
                return_info: None,
                doc: None,
            }],
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_closure_interface() {
        let template = ClosureInterfaceTemplate {
            interface_name: "OnProgress",
            params: &[
                KotlinSignatureParam {
                    name: "current".to_string(),
                    kotlin_type: "Int".to_string(),
                },
                KotlinSignatureParam {
                    name: "total".to_string(),
                    kotlin_type: "Int".to_string(),
                },
            ],
            return_type: "Unit",
            is_void_return: true,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_blittable_record() {
        let template = RecordTemplate {
            constructors: &[],
            methods: &[],
            class_name: "Point",
            fields: &[
                KotlinRecordField {
                    name: "x".to_string(),
                    kotlin_type: "Double".to_string(),
                    default_value: None,
                    wire_decode_expr: "reader.readF64()".to_string(),
                    wire_size_expr: "8".to_string(),
                    wire_encode: "wire.writeF64(x)".to_string(),
                    padding_after: 0,
                    doc: None,
                },
                KotlinRecordField {
                    name: "y".to_string(),
                    kotlin_type: "Double".to_string(),
                    default_value: None,
                    wire_decode_expr: "reader.readF64()".to_string(),
                    wire_size_expr: "8".to_string(),
                    wire_encode: "wire.writeF64(y)".to_string(),
                    padding_after: 0,
                    doc: None,
                },
            ],
            is_blittable: true,
            struct_size: 16,
            doc: &None,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_encoded_record_with_string() {
        let template = RecordTemplate {
            constructors: &[],
            methods: &[],
            class_name: "Person",
            fields: &[
                KotlinRecordField {
                    name: "id".to_string(),
                    kotlin_type: "Int".to_string(),
                    default_value: None,
                    wire_decode_expr: "reader.readI32()".to_string(),
                    wire_size_expr: "4".to_string(),
                    wire_encode: "wire.writeI32(id)".to_string(),
                    padding_after: 0,
                    doc: None,
                },
                KotlinRecordField {
                    name: "name".to_string(),
                    kotlin_type: "String".to_string(),
                    default_value: None,
                    wire_decode_expr: "reader.readString()".to_string(),
                    wire_size_expr: "wire.sizeString(name)".to_string(),
                    wire_encode: "wire.writeString(name)".to_string(),
                    padding_after: 0,
                    doc: None,
                },
            ],
            is_blittable: false,
            struct_size: 0,
            doc: &None,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_record_with_array_field() {
        let template = RecordTemplate {
            constructors: &[],
            methods: &[],
            class_name: "Team",
            fields: &[
                KotlinRecordField {
                    name: "name".to_string(),
                    kotlin_type: "String".to_string(),
                    default_value: None,
                    wire_decode_expr: "reader.readString()".to_string(),
                    wire_size_expr: "wire.sizeString(name)".to_string(),
                    wire_encode: "wire.writeString(name)".to_string(),
                    padding_after: 0,
                    doc: None,
                },
                KotlinRecordField {
                    name: "members".to_string(),
                    kotlin_type: "List<String>".to_string(),
                    default_value: None,
                    wire_decode_expr: "reader.readList { it.readString() }".to_string(),
                    wire_size_expr: "wire.sizeList(members) { w, v -> w.sizeString(v) }"
                        .to_string(),
                    wire_encode: "wire.writeList(members) { w, v -> w.writeString(v) }".to_string(),
                    padding_after: 0,
                    doc: None,
                },
            ],
            is_blittable: false,
            struct_size: 0,
            doc: &None,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_class_with_constructor_and_method() {
        let cls = KotlinClass {
            class_name: "Database".to_string(),
            doc: None,
            prefix: "boltffi".to_string(),
            ffi_free: "boltffi_database_free".to_string(),
            constructors: vec![KotlinConstructor {
                name: "Database".to_string(),
                is_factory: false,
                is_fallible: false,
                signature_params: vec![KotlinSignatureParam {
                    name: "path".to_string(),
                    kotlin_type: "String".to_string(),
                }],
                wire_writers: vec![KotlinWireWriter {
                    binding_name: "pathWire".to_string(),
                    size_expr: "BoltFFIWire.sizeString(path)".to_string(),
                    encode_expr: "BoltFFIWire.writeString(path)".to_string(),
                }],
                wire_writer_closes: vec!["pathWire.close()".to_string()],
                native_args: vec!["pathWire.ptr".to_string(), "pathWire.len".to_string()],
                ffi_name: "boltffi_database_open".to_string(),
                doc: None,
            }],
            methods: vec![KotlinMethod {
                impl_: KotlinMethodImpl::SyncMethod(
                    "fun query(sql: String): String { /* impl */ }".to_string(),
                ),
                is_static: false,
            }],
            streams: vec![],
            use_companion_methods: false,
        };
        let template = ClassTemplate {
            class_name: &cls.class_name,
            doc: &cls.doc,
            constructors: &cls.constructors,
            methods: &cls.methods,
            streams: &cls.streams,
            use_companion_methods: cls.use_companion_methods,
            has_factory_ctors: cls.has_factory_ctors(),
            has_static_methods: cls.has_static_methods(),
            prefix: &cls.prefix,
            ffi_free: &cls.ffi_free,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_class_with_nullable_handle_return() {
        let cls = KotlinClass {
            class_name: "Cache".to_string(),
            doc: None,
            prefix: "boltffi".to_string(),
            ffi_free: "boltffi_cache_free".to_string(),
            constructors: vec![],
            methods: vec![KotlinMethod {
                impl_: KotlinMethodImpl::SyncMethod(
                    "fun find(key: String): Cache? { val ptr = Native.boltffi_cache_find(handle, key); return if (ptr == 0L) null else Cache(ptr) }".to_string(),
                ),
                is_static: false,
            }],
            streams: vec![],
            use_companion_methods: false,
        };
        let template = ClassTemplate {
            class_name: &cls.class_name,
            doc: &cls.doc,
            constructors: &cls.constructors,
            methods: &cls.methods,
            streams: &cls.streams,
            use_companion_methods: cls.use_companion_methods,
            has_factory_ctors: cls.has_factory_ctors(),
            has_static_methods: cls.has_static_methods(),
            prefix: &cls.prefix,
            ffi_free: &cls.ffi_free,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_sync_function_with_record_param() {
        let template = WireFunctionTemplate {
            func_name: "processPoint",
            signature_params: &[KotlinSignatureParam {
                name: "point".to_string(),
                kotlin_type: "Point".to_string(),
            }],
            return_type: Some("Point"),
            wire_writers: &[KotlinWireWriter {
                binding_name: "pointWire".to_string(),
                size_expr: "Point.WIRE_SIZE".to_string(),
                encode_expr: "PointWriter.write(point)".to_string(),
            }],
            wire_writer_closes: &["pointWire.close()".to_string()],
            native_args: &["pointWire.ptr".to_string(), "pointWire.len".to_string()],
            throws: false,
            err_type: "",
            ffi_name: "boltffi_process_point",
            return_is_unit: false,
            return_is_direct: false,
            return_cast: "",
            decode_expr: "PointReader.read(reader)",
            is_blittable_return: true,
            doc: &None,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_sync_function_with_multiple_string_params() {
        let template = WireFunctionTemplate {
            func_name: "concat",
            signature_params: &[
                KotlinSignatureParam {
                    name: "a".to_string(),
                    kotlin_type: "String".to_string(),
                },
                KotlinSignatureParam {
                    name: "b".to_string(),
                    kotlin_type: "String".to_string(),
                },
            ],
            return_type: Some("String"),
            wire_writers: &[
                KotlinWireWriter {
                    binding_name: "aWire".to_string(),
                    size_expr: "BoltFFIWire.sizeString(a)".to_string(),
                    encode_expr: "BoltFFIWire.writeString(a)".to_string(),
                },
                KotlinWireWriter {
                    binding_name: "bWire".to_string(),
                    size_expr: "BoltFFIWire.sizeString(b)".to_string(),
                    encode_expr: "BoltFFIWire.writeString(b)".to_string(),
                },
            ],
            wire_writer_closes: &["aWire.close()".to_string(), "bWire.close()".to_string()],
            native_args: &[
                "aWire.ptr".to_string(),
                "aWire.len".to_string(),
                "bWire.ptr".to_string(),
                "bWire.len".to_string(),
            ],
            throws: false,
            err_type: "",
            ffi_name: "boltffi_concat",
            return_is_unit: false,
            return_is_direct: false,
            return_cast: "",
            decode_expr: "reader.readString()",
            is_blittable_return: false,
            doc: &None,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_sync_function_returning_optional() {
        let template = WireFunctionTemplate {
            func_name: "findUser",
            signature_params: &[KotlinSignatureParam {
                name: "id".to_string(),
                kotlin_type: "Int".to_string(),
            }],
            return_type: Some("String?"),
            wire_writers: &[],
            wire_writer_closes: &[],
            native_args: &["id".to_string()],
            throws: false,
            err_type: "",
            ffi_name: "boltffi_find_user",
            return_is_unit: false,
            return_is_direct: false,
            return_cast: "",
            decode_expr: "reader.readOption { it.readString() }",
            is_blittable_return: false,
            doc: &None,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_async_function_with_multiple_params() {
        let template = AsyncFunctionTemplate {
            func_name: "sendRequest",
            signature_params: &[
                KotlinSignatureParam {
                    name: "url".to_string(),
                    kotlin_type: "String".to_string(),
                },
                KotlinSignatureParam {
                    name: "body".to_string(),
                    kotlin_type: "ByteArray".to_string(),
                },
                KotlinSignatureParam {
                    name: "timeout".to_string(),
                    kotlin_type: "Int".to_string(),
                },
            ],
            return_type: Some("ByteArray"),
            wire_writers: &[
                KotlinWireWriter {
                    binding_name: "urlWire".to_string(),
                    size_expr: "BoltFFIWire.sizeString(url)".to_string(),
                    encode_expr: "BoltFFIWire.writeString(url)".to_string(),
                },
                KotlinWireWriter {
                    binding_name: "bodyWire".to_string(),
                    size_expr: "BoltFFIWire.sizeBytes(body)".to_string(),
                    encode_expr: "BoltFFIWire.writeBytes(body)".to_string(),
                },
            ],
            wire_writer_closes: &[
                "urlWire.close()".to_string(),
                "bodyWire.close()".to_string(),
            ],
            native_args: &[
                "urlWire.ptr".to_string(),
                "urlWire.len".to_string(),
                "bodyWire.ptr".to_string(),
                "bodyWire.len".to_string(),
                "timeout".to_string(),
            ],
            throws: false,
            err_type: "",
            ffi_name: "boltffi_send_request",
            include_handle: false,
            ffi_poll: "boltffi_send_request_poll",
            ffi_complete: "boltffi_send_request_complete",
            ffi_cancel: "boltffi_send_request_cancel",
            ffi_free: "boltffi_send_request_free",
            return_is_unit: false,
            return_is_direct: false,
            return_cast: "",
            decode_expr: "reader.readBytes()",
            is_blittable_return: false,
            doc: &None,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_data_enum_with_struct_payload() {
        let template = SealedEnumTemplate {
            constructors: &[],
            methods: &[],
            class_name: "Event",
            variants: &[
                KotlinEnumVariant {
                    name: "Click".to_string(),
                    tag: 0,
                    fields: vec![
                        KotlinEnumField {
                            name: "x".to_string(),
                            kotlin_type: "Int".to_string(),
                            wire_decode_expr: "reader.readI32()".to_string(),
                            wire_size_expr: "4".to_string(),
                            wire_encode: "wire.writeI32(x)".to_string(),
                        },
                        KotlinEnumField {
                            name: "y".to_string(),
                            kotlin_type: "Int".to_string(),
                            wire_decode_expr: "reader.readI32()".to_string(),
                            wire_size_expr: "4".to_string(),
                            wire_encode: "wire.writeI32(y)".to_string(),
                        },
                        KotlinEnumField {
                            name: "button".to_string(),
                            kotlin_type: "Int".to_string(),
                            wire_decode_expr: "reader.readI32()".to_string(),
                            wire_size_expr: "4".to_string(),
                            wire_encode: "wire.writeI32(button)".to_string(),
                        },
                    ],
                    doc: None,
                },
                KotlinEnumVariant {
                    name: "KeyPress".to_string(),
                    tag: 1,
                    fields: vec![KotlinEnumField {
                        name: "code".to_string(),
                        kotlin_type: "Int".to_string(),
                        wire_decode_expr: "reader.readI32()".to_string(),
                        wire_size_expr: "4".to_string(),
                        wire_encode: "wire.writeI32(code)".to_string(),
                    }],
                    doc: None,
                },
            ],
            is_error: false,
            doc: &None,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    fn snapshot_enum_with_associated_optional() {
        let template = SealedEnumTemplate {
            constructors: &[],
            methods: &[],
            class_name: "SearchResult",
            variants: &[
                KotlinEnumVariant {
                    name: "Found".to_string(),
                    tag: 0,
                    fields: vec![KotlinEnumField {
                        name: "item".to_string(),
                        kotlin_type: "String?".to_string(),
                        wire_decode_expr: "reader.readOption { it.readString() }".to_string(),
                        wire_size_expr: "wire.sizeOption(item) { w, v -> w.sizeString(v) }"
                            .to_string(),
                        wire_encode: "wire.writeOption(item) { w, v -> w.writeString(v) }"
                            .to_string(),
                    }],
                    doc: None,
                },
                KotlinEnumVariant {
                    name: "NotFound".to_string(),
                    tag: 1,
                    fields: vec![],
                    doc: None,
                },
            ],
            is_error: false,
            doc: &None,
        };
        insta::assert_snapshot!(template.render().unwrap());
    }
}
