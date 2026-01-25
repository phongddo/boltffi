use std::collections::HashMap;

use crate::ir::codec::{
    BlittableField, CodecPlan, EncodedField, EnumLayout, RecordLayout, VariantLayout,
    VariantPayloadLayout, VecLayout,
};
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{
    CallbackTraitDef, ClassDef, ConstructorDef, EnumRepr, FunctionDef, MethodDef, ParamDef,
    ParamPassing, Receiver, RecordDef, ReturnDef, VariantPayload,
};
use crate::ir::ids::{
    CallbackId, ClassId, EnumId, FieldName, FunctionId, MethodId, ParamName, RecordId,
};
use crate::ir::plan::{
    AbiType, AsyncPlan, AsyncResult, CallPlan, CallPlanKind, CallTarget, CallbackStyle,
    CompletionCallback, DirectPlan, Mutability, ParamPlan, ParamStrategy, ReturnPlan,
    ReturnValuePlan,
};
use crate::ir::types::{PrimitiveType, TypeExpr};

#[derive(Clone, Copy)]
pub struct Lowerer<'c> {
    contract: &'c FfiContract,
}

impl<'c> Lowerer<'c> {
    pub fn new(contract: &'c FfiContract) -> Self {
        Self { contract }
    }

    pub fn lower_function(&self, func: &FunctionDef) -> CallPlan {
        let params = func.params.iter().map(|p| self.lower_param(p)).collect();

        let kind = if func.is_async {
            CallPlanKind::Async {
                async_plan: self.build_async_plan(&func.returns),
            }
        } else {
            CallPlanKind::Sync {
                returns: self.lower_return(&func.returns),
            }
        };

        CallPlan {
            target: CallTarget::GlobalSymbol(self.function_symbol(&func.id)),
            params,
            kind,
        }
    }

    pub fn lower_method(&self, class: &ClassDef, method: &MethodDef) -> CallPlan {
        let mut params: Vec<ParamPlan> =
            method.params.iter().map(|p| self.lower_param(p)).collect();

        if method.receiver != Receiver::Static {
            params.insert(
                0,
                ParamPlan {
                    name: ParamName::new("self"),
                    strategy: ParamStrategy::Handle {
                        class_id: class.id.clone(),
                        nullable: false,
                    },
                },
            );
        }

        let kind = if method.is_async {
            CallPlanKind::Async {
                async_plan: self.build_async_plan(&method.returns),
            }
        } else {
            CallPlanKind::Sync {
                returns: self.lower_return(&method.returns),
            }
        };

        CallPlan {
            target: CallTarget::GlobalSymbol(self.method_symbol(&class.id, &method.id)),
            params,
            kind,
        }
    }

    pub fn lower_constructor(&self, class: &ClassDef, ctor: &ConstructorDef) -> CallPlan {
        let params = ctor.params.iter().map(|p| self.lower_param(p)).collect();

        let returns = if ctor.is_fallible {
            ReturnPlan::Fallible {
                ok: ReturnValuePlan::Handle {
                    class_id: class.id.clone(),
                    nullable: false,
                },
                err_codec: CodecPlan::String,
            }
        } else {
            ReturnPlan::Value(ReturnValuePlan::Handle {
                class_id: class.id.clone(),
                nullable: false,
            })
        };

        CallPlan {
            target: CallTarget::GlobalSymbol(
                self.constructor_symbol(&class.id, ctor.name.as_ref()),
            ),
            params,
            kind: CallPlanKind::Sync { returns },
        }
    }

    pub fn lower_callback(&self, callback: &CallbackTraitDef) -> Vec<CallPlan> {
        callback
            .methods
            .iter()
            .map(|method| {
                let mut params: Vec<ParamPlan> =
                    method.params.iter().map(|p| self.lower_param(p)).collect();

                params.insert(
                    0,
                    ParamPlan {
                        name: ParamName::new("callback"),
                        strategy: ParamStrategy::Callback {
                            callback_id: callback.id.clone(),
                            style: CallbackStyle::BoxedDyn,
                            nullable: false,
                        },
                    },
                );

                let kind = if method.is_async {
                    CallPlanKind::Async {
                        async_plan: self.build_async_plan(&method.returns),
                    }
                } else {
                    CallPlanKind::Sync {
                        returns: self.lower_return(&method.returns),
                    }
                };

                CallPlan {
                    target: CallTarget::VtableField(method.id.clone()),
                    params,
                    kind,
                }
            })
            .collect()
    }

    fn build_async_plan(&self, returns: &ReturnDef) -> AsyncPlan {
        let result = match returns {
            ReturnDef::Void => AsyncResult::Void,
            ReturnDef::Value(ty) => AsyncResult::Value(self.lower_value_type(ty)),
            ReturnDef::Result { ok, err } => AsyncResult::Fallible {
                ok: self.lower_value_type(ok),
                err_codec: self.build_codec(err),
            },
        };

        AsyncPlan {
            completion_callback: CompletionCallback {
                param_name: ParamName::new("completion"),
                ffi_type: AbiType::Pointer,
            },
            result,
        }
    }

    fn lower_param(&self, param: &ParamDef) -> ParamPlan {
        ParamPlan {
            name: param.name.clone(),
            strategy: self.param_strategy(&param.type_expr, &param.passing),
        }
    }

    fn param_strategy(&self, type_expr: &TypeExpr, passing: &ParamPassing) -> ParamStrategy {
        if let (ParamPassing::ImplTrait | ParamPassing::BoxedDyn, TypeExpr::Callback(id)) =
            (passing, type_expr)
        {
            let style = match passing {
                ParamPassing::ImplTrait => CallbackStyle::ImplTrait,
                ParamPassing::BoxedDyn => CallbackStyle::BoxedDyn,
                _ => unreachable!(),
            };
            return ParamStrategy::Callback {
                callback_id: id.clone(),
                style,
                nullable: false,
            };
        }

        let mutability = match passing {
            ParamPassing::RefMut => Mutability::Mutable,
            _ => Mutability::Shared,
        };

        match type_expr {
            TypeExpr::Void => ParamStrategy::Direct(DirectPlan {
                abi_type: AbiType::Void,
            }),

            TypeExpr::Primitive(p) => ParamStrategy::Direct(DirectPlan {
                abi_type: primitive_to_abi(*p),
            }),

            TypeExpr::String | TypeExpr::Bytes => ParamStrategy::Buffer {
                element_abi: AbiType::U8,
                mutability,
            },

            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(p) => ParamStrategy::Buffer {
                    element_abi: primitive_to_abi(*p),
                    mutability,
                },
                _ => ParamStrategy::Encoded {
                    codec: self.build_codec(type_expr),
                },
            },

            TypeExpr::Handle(class_id) => ParamStrategy::Handle {
                class_id: class_id.clone(),
                nullable: false,
            },

            TypeExpr::Option(inner) => match inner.as_ref() {
                TypeExpr::Handle(class_id) => ParamStrategy::Handle {
                    class_id: class_id.clone(),
                    nullable: true,
                },
                TypeExpr::Callback(callback_id) => ParamStrategy::Callback {
                    callback_id: callback_id.clone(),
                    style: CallbackStyle::BoxedDyn,
                    nullable: true,
                },
                _ => ParamStrategy::Encoded {
                    codec: self.build_codec(type_expr),
                },
            },

            TypeExpr::Callback(callback_id) => ParamStrategy::Callback {
                callback_id: callback_id.clone(),
                style: CallbackStyle::BoxedDyn,
                nullable: false,
            },

            _ => ParamStrategy::Encoded {
                codec: self.build_codec(type_expr),
            },
        }
    }

    fn lower_return(&self, returns: &ReturnDef) -> ReturnPlan {
        match returns {
            ReturnDef::Void => ReturnPlan::Value(ReturnValuePlan::Void),
            ReturnDef::Value(ty) => ReturnPlan::Value(self.lower_value_type(ty)),
            ReturnDef::Result { ok, err } => ReturnPlan::Fallible {
                ok: self.lower_value_type(ok),
                err_codec: self.build_codec(err),
            },
        }
    }

    fn lower_value_type(&self, ty: &TypeExpr) -> ReturnValuePlan {
        match ty {
            TypeExpr::Void => ReturnValuePlan::Void,

            TypeExpr::Primitive(p) => ReturnValuePlan::Direct(DirectPlan {
                abi_type: primitive_to_abi(*p),
            }),

            TypeExpr::Handle(class_id) => ReturnValuePlan::Handle {
                class_id: class_id.clone(),
                nullable: false,
            },

            TypeExpr::Callback(callback_id) => ReturnValuePlan::Callback {
                callback_id: callback_id.clone(),
                nullable: false,
            },

            TypeExpr::Option(inner) => match inner.as_ref() {
                TypeExpr::Handle(class_id) => ReturnValuePlan::Handle {
                    class_id: class_id.clone(),
                    nullable: true,
                },
                TypeExpr::Callback(callback_id) => ReturnValuePlan::Callback {
                    callback_id: callback_id.clone(),
                    nullable: true,
                },
                _ => ReturnValuePlan::Encoded {
                    codec: self.build_codec(ty),
                },
            },

            _ => ReturnValuePlan::Encoded {
                codec: self.build_codec(ty),
            },
        }
    }

    pub fn build_codec(&self, type_expr: &TypeExpr) -> CodecPlan {
        match type_expr {
            TypeExpr::Void => CodecPlan::Void,
            TypeExpr::Primitive(p) => CodecPlan::Primitive(*p),
            TypeExpr::String => CodecPlan::String,
            TypeExpr::Bytes => CodecPlan::Bytes,
            TypeExpr::Builtin(id) => CodecPlan::Builtin(id.clone()),

            TypeExpr::Option(inner) => CodecPlan::Option(Box::new(self.build_codec(inner))),

            TypeExpr::Vec(inner) => CodecPlan::Vec {
                element: Box::new(self.build_codec(inner)),
                layout: self.vec_layout(inner),
            },

            TypeExpr::Result { ok, err } => CodecPlan::Result {
                ok: Box::new(self.build_codec(ok)),
                err: Box::new(self.build_codec(err)),
            },

            TypeExpr::Record(id) => CodecPlan::Record {
                id: id.clone(),
                layout: self.record_layout(id),
            },

            TypeExpr::Enum(id) => CodecPlan::Enum {
                id: id.clone(),
                layout: self.enum_layout(id),
            },

            TypeExpr::Custom(id) => {
                let def = self
                    .contract
                    .catalog
                    .resolve_custom(id)
                    .expect("custom type should be resolved");
                CodecPlan::Custom {
                    id: id.clone(),
                    underlying: Box::new(self.build_codec(&def.repr)),
                }
            }

            TypeExpr::Handle(_) | TypeExpr::Callback(_) => {
                panic!("Handle and Callback types cannot be wire-encoded")
            }
        }
    }

    fn record_layout(&self, id: &RecordId) -> RecordLayout {
        let def = self
            .contract
            .catalog
            .resolve_record(id)
            .expect("record should be resolved");

        if self.is_blittable_record(def) {
            self.build_blittable_record_layout(def)
        } else {
            self.build_encoded_record_layout(def)
        }
    }

    fn is_blittable_record(&self, def: &RecordDef) -> bool {
        def.fields
            .iter()
            .all(|f| matches!(f.type_expr, TypeExpr::Primitive(_)))
    }

    fn build_blittable_record_layout(&self, def: &RecordDef) -> RecordLayout {
        let (size, fields) = compute_blittable_layout(def);
        RecordLayout::Blittable { size, fields }
    }

    fn build_encoded_record_layout(&self, def: &RecordDef) -> RecordLayout {
        let fields = def
            .fields
            .iter()
            .map(|f| EncodedField {
                name: f.name.clone(),
                codec: self.build_codec(&f.type_expr),
            })
            .collect();

        RecordLayout::Encoded { fields }
    }

    fn enum_layout(&self, id: &EnumId) -> EnumLayout {
        let def = self
            .contract
            .catalog
            .resolve_enum(id)
            .expect("enum should be resolved");

        match &def.repr {
            EnumRepr::CStyle { tag_type, .. } => EnumLayout::CStyle {
                tag_type: *tag_type,
            },

            EnumRepr::Data { tag_type, variants } => EnumLayout::Data {
                tag_type: *tag_type,
                variants: variants
                    .iter()
                    .map(|v| VariantLayout {
                        name: v.name.clone(),
                        discriminant: v.discriminant,
                        payload: self.variant_payload_layout(&v.payload),
                    })
                    .collect(),
            },
        }
    }

    fn variant_payload_layout(&self, payload: &VariantPayload) -> VariantPayloadLayout {
        match payload {
            VariantPayload::Unit => VariantPayloadLayout::Unit,
            VariantPayload::Tuple(types) => VariantPayloadLayout::Fields(
                types
                    .iter()
                    .enumerate()
                    .map(|(idx, ty)| EncodedField {
                        name: FieldName::new(format!("_{}", idx)),
                        codec: self.build_codec(ty),
                    })
                    .collect(),
            ),
            VariantPayload::Struct(fields) => VariantPayloadLayout::Fields(
                fields
                    .iter()
                    .map(|f| EncodedField {
                        name: f.name.clone(),
                        codec: self.build_codec(&f.type_expr),
                    })
                    .collect(),
            ),
        }
    }

    fn vec_layout(&self, element: &TypeExpr) -> VecLayout {
        match element {
            TypeExpr::Primitive(p) => VecLayout::Blittable {
                element_size: p.size_bytes(),
            },

            TypeExpr::Record(id) => {
                let def = self.contract.catalog.resolve_record(id);
                match def {
                    Some(def) if self.is_blittable_record(def) => VecLayout::Blittable {
                        element_size: self.blittable_record_size(def),
                    },
                    _ => VecLayout::Encoded,
                }
            }

            _ => VecLayout::Encoded,
        }
    }

    fn blittable_record_size(&self, def: &RecordDef) -> usize {
        let (size, _) = compute_blittable_layout(def);
        size
    }

    fn function_symbol(&self, id: &FunctionId) -> String {
        format!("{}_{}", self.contract.package.name, id.as_str())
    }

    fn method_symbol(&self, class_id: &ClassId, method_id: &MethodId) -> String {
        format!(
            "{}_{}_{}",
            self.contract.package.name,
            class_id.as_str(),
            method_id.as_str()
        )
    }

    fn constructor_symbol(&self, class_id: &ClassId, name: Option<&MethodId>) -> String {
        match name {
            Some(n) => format!(
                "{}_{}_{}",
                self.contract.package.name,
                class_id.as_str(),
                n.as_str()
            ),
            None => format!("{}_{}_new", self.contract.package.name, class_id.as_str()),
        }
    }
}

fn primitive_to_abi(p: PrimitiveType) -> AbiType {
    match p {
        PrimitiveType::Bool => AbiType::Bool,
        PrimitiveType::I8 => AbiType::I8,
        PrimitiveType::U8 => AbiType::U8,
        PrimitiveType::I16 => AbiType::I16,
        PrimitiveType::U16 => AbiType::U16,
        PrimitiveType::I32 => AbiType::I32,
        PrimitiveType::U32 => AbiType::U32,
        PrimitiveType::I64 => AbiType::I64,
        PrimitiveType::U64 => AbiType::U64,
        PrimitiveType::F32 => AbiType::F32,
        PrimitiveType::F64 => AbiType::F64,
    }
}

fn align_up(offset: usize, alignment: usize) -> usize {
    (offset + alignment - 1) & !(alignment - 1)
}

fn compute_blittable_layout(def: &RecordDef) -> (usize, Vec<BlittableField>) {
    let (final_offset, fields) =
        def.fields
            .iter()
            .fold((0usize, Vec::new()), |(offset, mut fields), field| {
                let TypeExpr::Primitive(p) = &field.type_expr else {
                    panic!("blittable record should only have primitive fields");
                };

                let aligned_offset = align_up(offset, p.alignment());

                fields.push(BlittableField {
                    name: field.name.clone(),
                    offset: aligned_offset,
                    primitive: *p,
                });

                (aligned_offset + p.size_bytes(), fields)
            });

    let max_align = def
        .fields
        .iter()
        .filter_map(|f| match &f.type_expr {
            TypeExpr::Primitive(p) => Some(p.alignment()),
            _ => None,
        })
        .max()
        .unwrap_or(1);

    let size = align_up(final_offset, max_align);
    (size, fields)
}

pub fn lower_contract(contract: &FfiContract) -> LoweredContract {
    let lowerer = Lowerer::new(contract);

    let functions = contract
        .functions
        .iter()
        .map(|f| (f.id.clone(), lowerer.lower_function(f)))
        .collect();

    let methods = contract
        .catalog
        .all_classes()
        .flat_map(|class| {
            class.methods.iter().map(move |m| {
                (
                    (class.id.clone(), m.id.clone()),
                    lowerer.lower_method(class, m),
                )
            })
        })
        .collect();

    let constructors =
        contract
            .catalog
            .all_classes()
            .flat_map(|class| {
                class.constructors.iter().enumerate().map(move |(idx, c)| {
                    ((class.id.clone(), idx), lowerer.lower_constructor(class, c))
                })
            })
            .collect();

    let callbacks = contract
        .catalog
        .all_callbacks()
        .map(|cb| (cb.id.clone(), lowerer.lower_callback(cb)))
        .collect();

    LoweredContract {
        functions,
        methods,
        constructors,
        callbacks,
    }
}

pub struct LoweredContract {
    pub functions: HashMap<FunctionId, CallPlan>,
    pub methods: HashMap<(ClassId, MethodId), CallPlan>,
    pub constructors: HashMap<(ClassId, usize), CallPlan>,
    pub callbacks: HashMap<CallbackId, Vec<CallPlan>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::contract::{FfiContract, PackageInfo, TypeCatalog};
    use crate::ir::definitions::{
        CallbackMethodDef, ClassDef, ConstructorDef, FieldDef, FunctionDef, MethodDef, ParamDef,
        ParamPassing, Receiver, RecordDef, ReturnDef,
    };
    use crate::ir::ids::{
        CallbackId, ClassId, FieldName, FunctionId, MethodId, ParamName, RecordId,
    };
    use crate::ir::types::{PrimitiveType, TypeExpr};

    fn test_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "test".to_string(),
                version: None,
            },
            catalog: TypeCatalog::default(),
            functions: vec![],
        }
    }

    fn lowerer_for_contract(contract: &FfiContract) -> Lowerer<'_> {
        Lowerer::new(contract)
    }

    #[test]
    fn param_strategy_primitive_is_direct() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.param_strategy(
            &TypeExpr::Primitive(PrimitiveType::I32),
            &ParamPassing::Value,
        );

        assert!(matches!(
            strategy,
            ParamStrategy::Direct(DirectPlan {
                abi_type: AbiType::I32
            })
        ));
    }

    #[test]
    fn param_strategy_string_is_buffer() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.param_strategy(&TypeExpr::String, &ParamPassing::Ref);

        assert!(matches!(
            strategy,
            ParamStrategy::Buffer {
                element_abi: AbiType::U8,
                mutability: Mutability::Shared
            }
        ));
    }

    #[test]
    fn param_strategy_vec_primitive_is_buffer() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.param_strategy(
            &TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::F32))),
            &ParamPassing::Ref,
        );

        assert!(matches!(
            strategy,
            ParamStrategy::Buffer {
                element_abi: AbiType::F32,
                mutability: Mutability::Shared
            }
        ));
    }

    #[test]
    fn param_strategy_handle_non_nullable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("MyClass");
        let strategy =
            lowerer.param_strategy(&TypeExpr::Handle(class_id.clone()), &ParamPassing::Value);

        assert!(matches!(
            strategy,
            ParamStrategy::Handle { class_id: ref id, nullable: false } if id.as_str() == "MyClass"
        ));
    }

    #[test]
    fn param_strategy_option_handle_is_nullable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("MyClass");
        let strategy = lowerer.param_strategy(
            &TypeExpr::Option(Box::new(TypeExpr::Handle(class_id.clone()))),
            &ParamPassing::Value,
        );

        assert!(matches!(
            strategy,
            ParamStrategy::Handle { class_id: ref id, nullable: true } if id.as_str() == "MyClass"
        ));
    }

    #[test]
    fn param_strategy_callback_impl_trait() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback_id = CallbackId::new("OnComplete");
        let strategy = lowerer.param_strategy(
            &TypeExpr::Callback(callback_id.clone()),
            &ParamPassing::ImplTrait,
        );

        assert!(matches!(
            strategy,
            ParamStrategy::Callback {
                callback_id: ref id,
                style: CallbackStyle::ImplTrait,
                nullable: false
            } if id.as_str() == "OnComplete"
        ));
    }

    #[test]
    fn param_strategy_option_callback_is_nullable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback_id = CallbackId::new("OnComplete");
        let strategy = lowerer.param_strategy(
            &TypeExpr::Option(Box::new(TypeExpr::Callback(callback_id.clone()))),
            &ParamPassing::Value,
        );

        assert!(matches!(
            strategy,
            ParamStrategy::Callback {
                callback_id: ref id,
                style: CallbackStyle::BoxedDyn,
                nullable: true
            } if id.as_str() == "OnComplete"
        ));
    }

    #[test]
    fn lower_return_void() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let plan = lowerer.lower_return(&ReturnDef::Void);

        assert!(matches!(plan, ReturnPlan::Value(ReturnValuePlan::Void)));
    }

    #[test]
    fn lower_return_primitive() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let plan =
            lowerer.lower_return(&ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::Bool)));

        assert!(matches!(
            plan,
            ReturnPlan::Value(ReturnValuePlan::Direct(DirectPlan {
                abi_type: AbiType::Bool
            }))
        ));
    }

    #[test]
    fn lower_return_handle() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Connection");
        let plan = lowerer.lower_return(&ReturnDef::Value(TypeExpr::Handle(class_id)));

        assert!(matches!(
            plan,
            ReturnPlan::Value(ReturnValuePlan::Handle {
                nullable: false,
                ..
            })
        ));
    }

    #[test]
    fn lower_return_option_handle_is_nullable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Connection");
        let plan = lowerer.lower_return(&ReturnDef::Value(TypeExpr::Option(Box::new(
            TypeExpr::Handle(class_id),
        ))));

        assert!(matches!(
            plan,
            ReturnPlan::Value(ReturnValuePlan::Handle { nullable: true, .. })
        ));
    }

    #[test]
    fn lower_return_result_handle_no_panic() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Connection");
        let plan = lowerer.lower_return(&ReturnDef::Result {
            ok: TypeExpr::Handle(class_id),
            err: TypeExpr::String,
        });

        assert!(matches!(
            plan,
            ReturnPlan::Fallible {
                ok: ReturnValuePlan::Handle {
                    nullable: false,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn lower_return_result_callback_no_panic() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback_id = CallbackId::new("Handler");
        let plan = lowerer.lower_return(&ReturnDef::Result {
            ok: TypeExpr::Callback(callback_id),
            err: TypeExpr::String,
        });

        assert!(matches!(
            plan,
            ReturnPlan::Fallible {
                ok: ReturnValuePlan::Callback {
                    nullable: false,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn build_codec_primitive() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Primitive(PrimitiveType::U64));

        assert!(matches!(codec, CodecPlan::Primitive(PrimitiveType::U64)));
    }

    #[test]
    fn build_codec_string() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::String);

        assert!(matches!(codec, CodecPlan::String));
    }

    #[test]
    fn build_codec_option() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Option(Box::new(TypeExpr::String)));

        assert!(matches!(codec, CodecPlan::Option(_)));
    }

    #[test]
    fn build_codec_vec_primitive_is_blittable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Vec(Box::new(TypeExpr::Primitive(
            PrimitiveType::I32,
        ))));

        assert!(matches!(
            codec,
            CodecPlan::Vec {
                layout: VecLayout::Blittable { element_size: 4 },
                ..
            }
        ));
    }

    #[test]
    fn build_codec_vec_string_is_encoded() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Vec(Box::new(TypeExpr::String)));

        assert!(matches!(
            codec,
            CodecPlan::Vec {
                layout: VecLayout::Encoded,
                ..
            }
        ));
    }

    #[test]
    #[should_panic(expected = "Handle and Callback types cannot be wire-encoded")]
    fn build_codec_handle_panics() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        lowerer.build_codec(&TypeExpr::Handle(ClassId::new("Foo")));
    }

    #[test]
    #[should_panic(expected = "Handle and Callback types cannot be wire-encoded")]
    fn build_codec_callback_panics() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        lowerer.build_codec(&TypeExpr::Callback(CallbackId::new("Bar")));
    }

    #[test]
    fn lower_function_sync() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let func = FunctionDef {
            id: FunctionId::new("greet"),
            params: vec![ParamDef {
                name: ParamName::new("name"),
                type_expr: TypeExpr::String,
                passing: ParamPassing::Ref,
                doc: None,
            }],
            returns: ReturnDef::Value(TypeExpr::String),
            is_async: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_function(&func);

        assert!(matches!(&plan.target, CallTarget::GlobalSymbol(s) if s == "test_greet"));
        assert_eq!(plan.params.len(), 1);
        assert!(matches!(plan.kind, CallPlanKind::Sync { .. }));
    }

    #[test]
    fn lower_function_async() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let func = FunctionDef {
            id: FunctionId::new("fetch"),
            params: vec![],
            returns: ReturnDef::Value(TypeExpr::String),
            is_async: true,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_function(&func);

        assert!(matches!(plan.kind, CallPlanKind::Async { .. }));
    }

    #[test]
    fn lower_method_inserts_self_handle() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Client");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let method = MethodDef {
            id: MethodId::new("connect"),
            receiver: Receiver::RefSelf,
            params: vec![],
            returns: ReturnDef::Void,
            is_async: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_method(class, &method);

        assert_eq!(plan.params.len(), 1);
        assert!(matches!(
            &plan.params[0].strategy,
            ParamStrategy::Handle {
                nullable: false,
                ..
            }
        ));
        assert_eq!(plan.params[0].name.as_str(), "self");
    }

    #[test]
    fn lower_method_static_no_self() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Utils");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let method = MethodDef {
            id: MethodId::new("helper"),
            receiver: Receiver::Static,
            params: vec![],
            returns: ReturnDef::Void,
            is_async: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_method(class, &method);

        assert_eq!(plan.params.len(), 0);
    }

    #[test]
    fn lower_constructor_non_fallible() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Builder");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let ctor = ConstructorDef {
            name: None,
            params: vec![],
            is_fallible: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_constructor(class, &ctor);

        assert!(matches!(
            plan.kind,
            CallPlanKind::Sync {
                returns: ReturnPlan::Value(ReturnValuePlan::Handle {
                    nullable: false,
                    ..
                })
            }
        ));
    }

    #[test]
    fn lower_constructor_fallible() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Parser");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let ctor = ConstructorDef {
            name: Some(MethodId::new("try_new")),
            params: vec![],
            is_fallible: true,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_constructor(class, &ctor);

        assert!(matches!(
            plan.kind,
            CallPlanKind::Sync {
                returns: ReturnPlan::Fallible { .. }
            }
        ));
    }

    #[test]
    fn lower_callback_uses_vtable_field() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback = CallbackTraitDef {
            id: CallbackId::new("EventHandler"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("on_event"),
                params: vec![],
                returns: ReturnDef::Void,
                is_async: false,
                doc: None,
            }],
            doc: None,
        };

        let plans = lowerer.lower_callback(&callback);

        assert_eq!(plans.len(), 1);
        assert!(matches!(
            &plans[0].target,
            CallTarget::VtableField(id) if id.as_str() == "on_event"
        ));
    }

    #[test]
    fn lower_callback_inserts_callback_handle() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback = CallbackTraitDef {
            id: CallbackId::new("Listener"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("notify"),
                params: vec![ParamDef {
                    name: ParamName::new("msg"),
                    type_expr: TypeExpr::String,
                    passing: ParamPassing::Ref,
                    doc: None,
                }],
                returns: ReturnDef::Void,
                is_async: false,
                doc: None,
            }],
            doc: None,
        };

        let plans = lowerer.lower_callback(&callback);

        assert_eq!(plans[0].params.len(), 2);
        assert_eq!(plans[0].params[0].name.as_str(), "callback");
        assert!(matches!(
            &plans[0].params[0].strategy,
            ParamStrategy::Callback {
                nullable: false,
                ..
            }
        ));
    }

    #[test]
    fn blittable_record_layout() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Point");
        contract.catalog.insert_record(RecordDef {
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F32),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F32),
                    doc: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let layout = lowerer.record_layout(&record_id);

        assert!(matches!(layout, RecordLayout::Blittable { size: 8, .. }));
    }

    #[test]
    fn encoded_record_layout() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Person");
        contract.catalog.insert_record(RecordDef {
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("name"),
                    type_expr: TypeExpr::String,
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("age"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U32),
                    doc: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let layout = lowerer.record_layout(&record_id);

        assert!(matches!(layout, RecordLayout::Encoded { .. }));
    }

    #[test]
    fn async_result_handles_result_handle() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Session");
        let async_plan = lowerer.build_async_plan(&ReturnDef::Result {
            ok: TypeExpr::Handle(class_id.clone()),
            err: TypeExpr::String,
        });

        match async_plan.result {
            AsyncResult::Fallible { ok, err_codec } => {
                match ok {
                    ReturnValuePlan::Handle {
                        class_id: id,
                        nullable,
                    } => {
                        assert_eq!(id.as_str(), "Session");
                        assert!(!nullable);
                    }
                    _ => panic!("expected Handle"),
                }
                assert!(matches!(err_codec, CodecPlan::String));
            }
            _ => panic!("expected Fallible"),
        }
    }

    #[test]
    fn param_strategy_vec_primitive_owned_is_buffer() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.param_strategy(
            &TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::U8))),
            &ParamPassing::Value,
        );

        match strategy {
            ParamStrategy::Buffer {
                element_abi,
                mutability,
            } => {
                assert_eq!(element_abi, AbiType::U8);
                assert_eq!(mutability, Mutability::Shared);
            }
            _ => panic!("expected Buffer for owned Vec<primitive>"),
        }
    }

    #[test]
    fn param_strategy_ref_mut_has_mutable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.param_strategy(&TypeExpr::String, &ParamPassing::RefMut);

        match strategy {
            ParamStrategy::Buffer {
                element_abi,
                mutability,
            } => {
                assert_eq!(element_abi, AbiType::U8);
                assert_eq!(mutability, Mutability::Mutable);
            }
            _ => panic!("expected Buffer"),
        }
    }

    #[test]
    fn param_strategy_bytes_is_buffer() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.param_strategy(&TypeExpr::Bytes, &ParamPassing::Ref);

        match strategy {
            ParamStrategy::Buffer {
                element_abi,
                mutability,
            } => {
                assert_eq!(element_abi, AbiType::U8);
                assert_eq!(mutability, Mutability::Shared);
            }
            _ => panic!("expected Buffer for Bytes"),
        }
    }

    #[test]
    fn lower_constructor_fallible_verifies_ok_and_err() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Connection");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let ctor = ConstructorDef {
            name: Some(MethodId::new("connect")),
            params: vec![],
            is_fallible: true,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_constructor(class, &ctor);

        match plan.kind {
            CallPlanKind::Sync {
                returns: ReturnPlan::Fallible { ok, err_codec },
            } => {
                match ok {
                    ReturnValuePlan::Handle {
                        class_id: id,
                        nullable,
                    } => {
                        assert_eq!(id.as_str(), "Connection");
                        assert!(!nullable);
                    }
                    _ => panic!("expected Handle in ok"),
                }
                assert!(matches!(err_codec, CodecPlan::String));
            }
            _ => panic!("expected Sync Fallible"),
        }
    }

    #[test]
    fn blittable_record_layout_verifies_offsets() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Packed");
        contract.catalog.insert_record(RecordDef {
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("a"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U8),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("b"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U32),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("c"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U8),
                    doc: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let layout = lowerer.record_layout(&record_id);

        match layout {
            RecordLayout::Blittable { size, fields } => {
                assert_eq!(size, 12);
                assert_eq!(fields.len(), 3);
                assert_eq!(fields[0].name.as_str(), "a");
                assert_eq!(fields[0].offset, 0);
                assert_eq!(fields[0].primitive, PrimitiveType::U8);
                assert_eq!(fields[1].name.as_str(), "b");
                assert_eq!(fields[1].offset, 4);
                assert_eq!(fields[1].primitive, PrimitiveType::U32);
                assert_eq!(fields[2].name.as_str(), "c");
                assert_eq!(fields[2].offset, 8);
                assert_eq!(fields[2].primitive, PrimitiveType::U8);
            }
            _ => panic!("expected Blittable"),
        }
    }

    #[test]
    fn vec_blittable_record_is_blittable() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Vec2");
        contract.catalog.insert_record(RecordDef {
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let codec = lowerer.build_codec(&TypeExpr::Vec(Box::new(TypeExpr::Record(record_id))));

        match codec {
            CodecPlan::Vec { element, layout } => {
                assert!(matches!(layout, VecLayout::Blittable { element_size: 16 }));
                assert!(matches!(*element, CodecPlan::Record { .. }));
            }
            _ => panic!("expected Vec"),
        }
    }

    #[test]
    fn build_codec_result() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Result {
            ok: Box::new(TypeExpr::Primitive(PrimitiveType::I64)),
            err: Box::new(TypeExpr::String),
        });

        match codec {
            CodecPlan::Result { ok, err } => {
                assert!(matches!(*ok, CodecPlan::Primitive(PrimitiveType::I64)));
                assert!(matches!(*err, CodecPlan::String));
            }
            _ => panic!("expected Result"),
        }
    }

    #[test]
    fn lower_return_verifies_class_id() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Database");
        let plan = lowerer.lower_return(&ReturnDef::Value(TypeExpr::Handle(class_id)));

        match plan {
            ReturnPlan::Value(ReturnValuePlan::Handle { class_id, nullable }) => {
                assert_eq!(class_id.as_str(), "Database");
                assert!(!nullable);
            }
            _ => panic!("expected Value Handle"),
        }
    }

    #[test]
    fn lower_callback_verifies_callback_id() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback = CallbackTraitDef {
            id: CallbackId::new("MyCallback"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("invoke"),
                params: vec![],
                returns: ReturnDef::Void,
                is_async: false,
                doc: None,
            }],
            doc: None,
        };

        let plans = lowerer.lower_callback(&callback);

        match &plans[0].params[0].strategy {
            ParamStrategy::Callback {
                callback_id,
                style,
                nullable,
            } => {
                assert_eq!(callback_id.as_str(), "MyCallback");
                assert_eq!(*style, CallbackStyle::BoxedDyn);
                assert!(!nullable);
            }
            _ => panic!("expected Callback strategy"),
        }
    }

    #[test]
    fn param_strategy_callback_boxed_dyn() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback_id = CallbackId::new("Handler");
        let strategy = lowerer.param_strategy(
            &TypeExpr::Callback(callback_id.clone()),
            &ParamPassing::BoxedDyn,
        );

        match strategy {
            ParamStrategy::Callback {
                callback_id: id,
                style,
                nullable,
            } => {
                assert_eq!(id.as_str(), "Handler");
                assert_eq!(style, CallbackStyle::BoxedDyn);
                assert!(!nullable);
            }
            _ => panic!("expected Callback"),
        }
    }

    #[test]
    fn lower_method_verifies_symbol() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Service");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let method = MethodDef {
            id: MethodId::new("start"),
            receiver: Receiver::RefMutSelf,
            params: vec![],
            returns: ReturnDef::Void,
            is_async: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_method(class, &method);

        match &plan.target {
            CallTarget::GlobalSymbol(s) => {
                assert_eq!(s, "test_Service_start");
            }
            _ => panic!("expected GlobalSymbol"),
        }
    }

    #[test]
    fn lower_constructor_verifies_symbol() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Factory");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let class = contract.catalog.resolve_class(&class_id).unwrap();

        let default_ctor = ConstructorDef {
            name: None,
            params: vec![],
            is_fallible: false,
            doc: None,
            deprecated: None,
        };
        let plan = lowerer.lower_constructor(class, &default_ctor);
        match &plan.target {
            CallTarget::GlobalSymbol(s) => assert_eq!(s, "test_Factory_new"),
            _ => panic!("expected GlobalSymbol"),
        }

        let named_ctor = ConstructorDef {
            name: Some(MethodId::new("with_config")),
            params: vec![],
            is_fallible: false,
            doc: None,
            deprecated: None,
        };
        let plan = lowerer.lower_constructor(class, &named_ctor);
        match &plan.target {
            CallTarget::GlobalSymbol(s) => assert_eq!(s, "test_Factory_with_config"),
            _ => panic!("expected GlobalSymbol"),
        }
    }

    #[test]
    fn encoded_record_verifies_field_codecs() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Message");
        contract.catalog.insert_record(RecordDef {
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("id"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U64),
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("body"),
                    type_expr: TypeExpr::String,
                    doc: None,
                },
                FieldDef {
                    name: FieldName::new("tags"),
                    type_expr: TypeExpr::Vec(Box::new(TypeExpr::String)),
                    doc: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let layout = lowerer.record_layout(&record_id);

        match layout {
            RecordLayout::Encoded { fields } => {
                assert_eq!(fields.len(), 3);
                assert_eq!(fields[0].name.as_str(), "id");
                assert!(matches!(
                    fields[0].codec,
                    CodecPlan::Primitive(PrimitiveType::U64)
                ));
                assert_eq!(fields[1].name.as_str(), "body");
                assert!(matches!(fields[1].codec, CodecPlan::String));
                assert_eq!(fields[2].name.as_str(), "tags");
                assert!(matches!(fields[2].codec, CodecPlan::Vec { .. }));
            }
            _ => panic!("expected Encoded"),
        }
    }
}
