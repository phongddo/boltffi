use askama::Template;

use super::plan::{
    JavaCallbackTrait, JavaClass, JavaClosureInterface, JavaEnum, JavaModule, JavaRecord,
};

#[derive(Template)]
#[template(path = "render_java/preamble.txt", escape = "none")]
pub struct PreambleTemplate<'a> {
    pub module: &'a JavaModule,
}

#[derive(Template)]
#[template(path = "render_java/record.txt", escape = "none")]
pub struct RecordTemplate<'a> {
    pub record: &'a JavaRecord,
    pub package_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_java/native.txt", escape = "none")]
pub struct NativeTemplate<'a> {
    pub module: &'a JavaModule,
}

#[derive(Template)]
#[template(path = "render_java/functions.txt", escape = "none")]
pub struct FunctionsTemplate<'a> {
    pub module: &'a JavaModule,
}

#[derive(Template)]
#[template(path = "render_java/enum_c_style.txt", escape = "none")]
pub struct CStyleEnumTemplate<'a> {
    pub enumeration: &'a JavaEnum,
    pub package_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_java/enum_error.txt", escape = "none")]
pub struct ErrorEnumTemplate<'a> {
    pub enumeration: &'a JavaEnum,
    pub package_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_java/enum_sealed.txt", escape = "none")]
pub struct DataEnumSealedTemplate<'a> {
    pub enumeration: &'a JavaEnum,
    pub package_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_java/enum_abstract.txt", escape = "none")]
pub struct DataEnumAbstractTemplate<'a> {
    pub enumeration: &'a JavaEnum,
    pub package_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_java/class.txt", escape = "none")]
pub struct ClassTemplate<'a> {
    pub class: &'a JavaClass,
    pub package_name: &'a str,
    pub async_mode: &'a super::plan::JavaAsyncMode,
}

#[derive(Template)]
#[template(path = "render_java/closure.txt", escape = "none")]
pub struct ClosureTemplate<'a> {
    pub closure: &'a JavaClosureInterface,
    pub package_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_java/callback_trait.txt", escape = "none")]
pub struct CallbackTraitTemplate<'a> {
    pub callback: &'a JavaCallbackTrait,
    pub package_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_java/closure_callbacks.txt", escape = "none")]
pub struct ClosureCallbacksTemplate<'a> {
    pub closure: &'a JavaClosureInterface,
    pub package_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_java/callback_callbacks.txt", escape = "none")]
pub struct CallbackCallbacksTemplate<'a> {
    pub callback: &'a JavaCallbackTrait,
    pub package_name: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::java::JavaVersion;
    use crate::render::java::plan::{
        JavaAsyncMode, JavaClassMethod, JavaConstructor, JavaConstructorKind, JavaFunction,
        JavaParam, JavaReturnPlan, JavaReturnRender, JavaWireWriter,
    };

    fn java_param(name: &str, java_type: &str, native_type: &str, native_expr: &str) -> JavaParam {
        JavaParam {
            name: name.to_string(),
            java_type: java_type.to_string(),
            native_type: native_type.to_string(),
            native_expr: native_expr.to_string(),
        }
    }

    fn wire_writer(
        binding_name: &str,
        param_name: &str,
        size_expr: &str,
        encode_expr: &str,
    ) -> JavaWireWriter {
        JavaWireWriter {
            binding_name: binding_name.to_string(),
            param_name: param_name.to_string(),
            size_expr: size_expr.to_string(),
            encode_expr: encode_expr.to_string(),
        }
    }

    fn java_module(classes: Vec<JavaClass>) -> JavaModule {
        JavaModule {
            package_name: "com.test".to_string(),
            class_name: "Test".to_string(),
            lib_name: "test".to_string(),
            java_version: JavaVersion::JAVA_17,
            async_mode: JavaAsyncMode::CompletableFuture,
            prefix: "boltffi".to_string(),
            records: vec![],
            enums: vec![],
            closures: vec![],
            callbacks: vec![],
            async_callback_invokers: vec![],
            functions: vec![],
            classes,
        }
    }

    #[test]
    fn class_template_renders_nullable_handle_return_guard() {
        let class = JavaClass {
            class_name: "Node".to_string(),
            ffi_free: "boltffi_node_free".to_string(),
            constructors: vec![],
            methods: vec![JavaClassMethod {
                name: "maybeNext".to_string(),
                ffi_name: "boltffi_node_maybe_next".to_string(),
                is_static: false,
                params: vec![],
                return_type: "Node".to_string(),
                return_plan: JavaReturnPlan {
                    native_return_type: "long".to_string(),
                    render: JavaReturnRender::Handle {
                        class_name: "Node".to_string(),
                        nullable: true,
                    },
                },
                wire_writers: vec![],
                async_call: None,
            }],
        };

        let source = ClassTemplate {
            class: &class,
            package_name: "com.test",
            async_mode: &JavaAsyncMode::CompletableFuture,
        }
        .render()
        .expect("class template should render");

        assert!(source.contains("if (_handle == 0L) return null;"));
        assert!(source.contains("return new Node(_handle);"));
    }

    #[test]
    fn class_template_renders_c_style_enum_decode_for_wire_methods() {
        let payload_param = java_param(
            "payload",
            "Payload",
            "ByteBuffer",
            "_wire_payload.toBuffer()",
        );
        let payload_writer = wire_writer(
            "_wire_payload",
            "payload",
            "8",
            "encodePayload(_wire_payload)",
        );

        let class = JavaClass {
            class_name: "Counter".to_string(),
            ffi_free: "boltffi_counter_free".to_string(),
            constructors: vec![],
            methods: vec![
                JavaClassMethod {
                    name: "fromPayload".to_string(),
                    ffi_name: "boltffi_counter_from_payload".to_string(),
                    is_static: true,
                    params: vec![payload_param.clone()],
                    return_type: "Status".to_string(),
                    return_plan: JavaReturnPlan {
                        native_return_type: "int".to_string(),
                        render: JavaReturnRender::CStyleEnum {
                            class_name: "Status".to_string(),
                        },
                    },
                    wire_writers: vec![payload_writer.clone()],
                    async_call: None,
                },
                JavaClassMethod {
                    name: "stateWithPayload".to_string(),
                    ffi_name: "boltffi_counter_state_with_payload".to_string(),
                    is_static: false,
                    params: vec![payload_param],
                    return_type: "Status".to_string(),
                    return_plan: JavaReturnPlan {
                        native_return_type: "int".to_string(),
                        render: JavaReturnRender::CStyleEnum {
                            class_name: "Status".to_string(),
                        },
                    },
                    wire_writers: vec![payload_writer],
                    async_call: None,
                },
            ],
        };

        let source = ClassTemplate {
            class: &class,
            package_name: "com.test",
            async_mode: &JavaAsyncMode::CompletableFuture,
        }
        .render()
        .expect("class template should render");

        assert!(
            source.contains(
                "return Status.fromValue(Native.boltffi_counter_from_payload(_wire_payload.toBuffer()));"
            )
        );
        assert!(
            source.contains(
                "return Status.fromValue(Native.boltffi_counter_state_with_payload(handle, _wire_payload.toBuffer()));"
            )
        );
    }

    #[test]
    fn native_template_renders_class_native_declarations() {
        let class = JavaClass {
            class_name: "Counter".to_string(),
            ffi_free: "boltffi_counter_free".to_string(),
            constructors: vec![JavaConstructor {
                kind: JavaConstructorKind::Primary,
                name: String::new(),
                is_fallible: false,
                params: vec![],
                ffi_name: "boltffi_counter_new".to_string(),
                wire_writers: vec![],
            }],
            methods: vec![
                JavaClassMethod {
                    name: "globalCount".to_string(),
                    ffi_name: "boltffi_counter_global_count".to_string(),
                    is_static: true,
                    params: vec![],
                    return_type: "int".to_string(),
                    return_plan: JavaReturnPlan {
                        native_return_type: "int".to_string(),
                        render: JavaReturnRender::Direct,
                    },
                    wire_writers: vec![],
                    async_call: None,
                },
                JavaClassMethod {
                    name: "get".to_string(),
                    ffi_name: "boltffi_counter_get".to_string(),
                    is_static: false,
                    params: vec![],
                    return_type: "int".to_string(),
                    return_plan: JavaReturnPlan {
                        native_return_type: "int".to_string(),
                        render: JavaReturnRender::Direct,
                    },
                    wire_writers: vec![],
                    async_call: None,
                },
            ],
        };
        let module = JavaModule {
            functions: vec![JavaFunction {
                name: "noop".to_string(),
                ffi_name: "boltffi_noop".to_string(),
                params: vec![],
                return_type: "void".to_string(),
                return_plan: JavaReturnPlan {
                    native_return_type: "void".to_string(),
                    render: JavaReturnRender::Void,
                },
                wire_writers: vec![],
                async_call: None,
            }],
            ..java_module(vec![class])
        };

        let source = NativeTemplate { module: &module }
            .render()
            .expect("native template should render");

        assert!(source.contains("static native void boltffi_counter_free(long handle);"));
        assert!(source.contains("static native long boltffi_counter_new();"));
        assert!(source.contains("static native int boltffi_counter_global_count();"));
        assert!(source.contains("static native int boltffi_counter_get(long handle);"));
    }
}
