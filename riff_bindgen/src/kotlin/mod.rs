mod marshal;
mod names;
mod templates;
mod types;

use askama::Template;

pub use marshal::{ParamConversion, ReturnKind};
pub use names::NamingConvention;
pub use templates::{
    CallbackTraitTemplate, ClassTemplate, CStyleEnumTemplate, FunctionTemplate, NativeTemplate,
    PreambleTemplate, RecordTemplate, SealedEnumTemplate,
};
pub use types::TypeMapper;

use crate::model::{CallbackTrait, Class, Enumeration, Function, Module, Record};

pub struct Kotlin;

impl Kotlin {
    pub fn render_module(module: &Module) -> String {
        let mut sections = Vec::new();

        sections.push(Self::render_preamble(module));

        module
            .enums
            .iter()
            .for_each(|enumeration| sections.push(Self::render_enum(enumeration)));

        module
            .records
            .iter()
            .for_each(|record| sections.push(Self::render_record(record)));

        module
            .functions
            .iter()
            .for_each(|function| sections.push(Self::render_function(function)));

        module
            .classes
            .iter()
            .for_each(|class| sections.push(Self::render_class(class)));

        module
            .callback_traits
            .iter()
            .for_each(|cb| sections.push(Self::render_callback_trait(cb, module)));

        sections.push(Self::render_native(module));

        let mut output = sections
            .into_iter()
            .map(|section| section.trim().to_string())
            .filter(|section| !section.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        output.push('\n');
        output
    }

    pub fn render_preamble(module: &Module) -> String {
        PreambleTemplate::from_module(module)
            .render()
            .expect("preamble template failed")
    }

    pub fn render_enum(enumeration: &Enumeration) -> String {
        if enumeration.is_c_style() {
            CStyleEnumTemplate::from_enum(enumeration)
                .render()
                .expect("c-style enum template failed")
        } else {
            SealedEnumTemplate::from_enum(enumeration)
                .render()
                .expect("sealed enum template failed")
        }
    }

    pub fn render_record(record: &Record) -> String {
        RecordTemplate::from_record(record)
            .render()
            .expect("record template failed")
    }

    pub fn render_function(function: &Function) -> String {
        FunctionTemplate::from_function(function)
            .render()
            .expect("function template failed")
    }

    pub fn render_class(class: &Class) -> String {
        ClassTemplate::from_class(class)
            .render()
            .expect("class template failed")
    }

    pub fn render_native(module: &Module) -> String {
        NativeTemplate::from_module(module)
            .render()
            .expect("native template failed")
    }

    pub fn render_callback_trait(callback_trait: &CallbackTrait, module: &Module) -> String {
        CallbackTraitTemplate::from_trait(callback_trait, module)
            .render()
            .expect("callback trait template failed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Constructor, Method, Module, Parameter, Primitive, Receiver, RecordField, TraitMethod,
        TraitMethodParam, Type, Variant,
    };

    #[test]
    fn test_kotlin_type_mapping() {
        assert_eq!(
            TypeMapper::map_type(&Type::Primitive(Primitive::I32)),
            "Int"
        );
        assert_eq!(
            TypeMapper::map_type(&Type::Primitive(Primitive::I64)),
            "Long"
        );
        assert_eq!(
            TypeMapper::map_type(&Type::Primitive(Primitive::Bool)),
            "Boolean"
        );
        assert_eq!(TypeMapper::map_type(&Type::String), "String");
        assert_eq!(TypeMapper::map_type(&Type::Bytes), "ByteArray");
        assert_eq!(
            TypeMapper::map_type(&Type::Vec(Box::new(Type::Primitive(Primitive::F64)))),
            "List<Double>"
        );
    }

    #[test]
    fn test_kotlin_naming() {
        assert_eq!(
            NamingConvention::class_name("sensor_manager"),
            "SensorManager"
        );
        assert_eq!(NamingConvention::method_name("get_reading"), "getReading");
        assert_eq!(NamingConvention::enum_entry_name("active"), "ACTIVE");
    }

    #[test]
    fn test_kotlin_keyword_escaping() {
        assert_eq!(NamingConvention::escape_keyword("value"), "`value`");
        assert_eq!(NamingConvention::escape_keyword("count"), "count");
    }

    #[test]
    fn test_render_c_style_enum() {
        let status = Enumeration::new("sensor_status")
            .with_variant(Variant::new("idle").with_discriminant(0))
            .with_variant(Variant::new("active").with_discriminant(1))
            .with_variant(Variant::new("error").with_discriminant(2));

        let output = Kotlin::render_enum(&status);
        assert!(output.contains("enum class SensorStatus"));
        assert!(output.contains("IDLE(0)"));
        assert!(output.contains("ACTIVE(1)"));
        assert!(output.contains("fromValue(value: Int)"));
    }

    #[test]
    fn test_render_sealed_class_enum() {
        let result_enum = Enumeration::new("api_result")
            .with_variant(Variant::new("success"))
            .with_variant(
                Variant::new("error")
                    .with_field(RecordField::new("code", Type::Primitive(Primitive::I32))),
            );

        let output = Kotlin::render_enum(&result_enum);
        assert!(output.contains("sealed class ApiResult"));
        assert!(output.contains("data object Success"));
        assert!(output.contains("data class Error"));
        assert!(output.contains("val code: Int"));
    }

    #[test]
    fn test_render_record() {
        let reading = Record::new("sensor_reading")
            .with_field(RecordField::new(
                "timestamp",
                Type::Primitive(Primitive::U64),
            ))
            .with_field(RecordField::new(
                "temperature",
                Type::Primitive(Primitive::F64),
            ));

        let output = Kotlin::render_record(&reading);
        assert!(output.contains("data class SensorReading"));
        assert!(output.contains("val timestamp: ULong"));
        assert!(output.contains("val temperature: Double"));
    }

    #[test]
    fn test_render_function() {
        let function = Function::new("get_sensor_value")
            .with_param(Parameter::new("sensor_id", Type::Primitive(Primitive::I32)))
            .with_output(Type::Primitive(Primitive::F64));

        let output = Kotlin::render_function(&function);
        assert!(output.contains("fun getSensorValue"));
        assert!(output.contains("sensorId: Int"));
        assert!(output.contains(": Double"));
    }

    #[test]
    fn test_render_class() {
        let sensor_class = Class::new("sensor")
            .with_constructor(Constructor::new())
            .with_method(
                Method::new("get_reading", Receiver::Ref)
                    .with_output(Type::Primitive(Primitive::F64)),
            );

        let output = Kotlin::render_class(&sensor_class);
        assert!(output.contains("class Sensor"));
        assert!(output.contains("private val handle: Long"));
        assert!(output.contains("override fun close()"));
        assert!(output.contains("fun getReading()"));
    }

    #[test]
    fn test_render_async_function() {
        let function = Function::new("fetch_data")
            .with_output(Type::String)
            .make_async();

        let output = Kotlin::render_function(&function);
        assert!(output.contains("suspend fun fetchData"));
        assert!(output.contains("suspendCancellableCoroutine"));
        assert!(output.contains("FfiCallback"));
    }

    #[test]
    fn test_render_callback_trait() {
        let callback = CallbackTrait::new("data_handler")
            .with_method(
                TraitMethod::new("on_data")
                    .with_param(TraitMethodParam::new("data", Type::Bytes)),
            )
            .with_method(TraitMethod::new("on_error").with_param(TraitMethodParam::new(
                "code",
                Type::Primitive(Primitive::I32),
            )));

        let module = Module::new("test");
        let output = Kotlin::render_callback_trait(&callback, &module);
        assert!(output.contains("interface DataHandler"));
        assert!(output.contains("fun onData(`data`: ByteArray)"));
        assert!(output.contains("fun onError(code: Int)"));
        assert!(output.contains("DataHandlerBridge"));
    }

    #[test]
    fn test_render_native() {
        let module = Module::new("mylib")
            .with_function(
                Function::new("get_version").with_output(Type::Primitive(Primitive::I32)),
            )
            .with_class(
                Class::new("sensor")
                    .with_constructor(Constructor::new())
                    .with_method(Method::new("read", Receiver::Ref)),
            );

        let output = Kotlin::render_native(&module);
        assert!(output.contains("interface NativeLib : Library"));
        assert!(output.contains("riff_get_version"));
        assert!(output.contains("riff_sensor_new"));
        assert!(output.contains("riff_sensor_free"));
        assert!(output.contains("riff_sensor_read"));
        assert!(output.contains("riff_cancel_async"));
    }
}
