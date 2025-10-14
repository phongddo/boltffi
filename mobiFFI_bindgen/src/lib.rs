pub mod model;
pub mod kotlin;
pub mod swift;

pub use model::{
    Class, Constructor, ConstructorParam, Deprecation, Enumeration, Function, Method, Module,
    Parameter, Primitive, Receiver, Record, RecordField, StreamMethod, StreamMode, Type, Variant,
};

pub use kotlin::Kotlin;
pub use swift::Swift;

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_module() -> Module {
        let sensor_class = Class::new("Sensor")
            .with_doc("A hardware sensor interface")
            .with_constructor(Constructor::new())
            .with_method(
                Method::new("get_reading", Receiver::Ref)
                    .with_output(Type::Primitive(Primitive::F64)),
            )
            .with_method(
                Method::new("predict_next", Receiver::Ref)
                    .with_param(Parameter::new("samples", Type::Primitive(Primitive::U32)))
                    .with_output(Type::vec(Type::Primitive(Primitive::F64)))
                    .make_async(),
            )
            .with_stream(StreamMethod::new(
                "readings",
                Type::Primitive(Primitive::F64),
            ));

        let reading_record = Record::new("SensorReading")
            .with_field(RecordField::new("timestamp", Type::Primitive(Primitive::U64)))
            .with_field(RecordField::new("value", Type::Primitive(Primitive::F64)))
            .with_field(RecordField::new("unit", Type::String));

        let status_enum = Enumeration::new("SensorStatus")
            .with_variant(Variant::new("idle").with_discriminant(0))
            .with_variant(Variant::new("active").with_discriminant(1))
            .with_variant(Variant::new("error").with_discriminant(2));

        Module::new("sensors")
            .with_class(sensor_class)
            .with_record(reading_record)
            .with_enum(status_enum)
    }

    #[test]
    fn test_module_ffi_prefix() {
        let module = create_test_module();
        assert_eq!(module.ffi_prefix(), "mffi");
    }

    #[test]
    fn test_class_ffi_names() {
        let module = create_test_module();
        let class = module.find_class("Sensor").unwrap();
        let prefix = module.ffi_prefix();

        assert_eq!(class.ffi_new(&prefix), "mffi_sensor_new");
        assert_eq!(class.ffi_free(&prefix), "mffi_sensor_free");
    }

    #[test]
    fn test_method_ffi_names() {
        let module = create_test_module();
        let class = module.find_class("Sensor").unwrap();
        let class_prefix = class.ffi_prefix(&module.ffi_prefix());
        let method = class.methods.iter().find(|m| m.name == "predict_next").unwrap();

        assert_eq!(method.ffi_name(&class_prefix), "mffi_sensor_predict_next");
        assert_eq!(method.ffi_poll(&class_prefix), "mffi_sensor_predict_next_poll");
    }

    #[test]
    fn test_swift_type_mapping() {
        use swift::TypeMapper;
        assert_eq!(TypeMapper::map_type(&Type::Primitive(Primitive::I32)), "Int32");
        assert_eq!(TypeMapper::map_type(&Type::Primitive(Primitive::Bool)), "Bool");
        assert_eq!(TypeMapper::map_type(&Type::String), "String");
        assert_eq!(TypeMapper::map_type(&Type::Bytes), "Data");
        assert_eq!(TypeMapper::map_type(&Type::vec(Type::Primitive(Primitive::F64))), "[Double]");
        assert_eq!(TypeMapper::map_type(&Type::option(Type::String)), "String?");
    }

    #[test]
    fn test_swift_naming_convention() {
        use swift::NamingConvention;
        assert_eq!(NamingConvention::class_name("sensor_manager"), "SensorManager");
        assert_eq!(NamingConvention::method_name("get_current_reading"), "getCurrentReading");
        assert_eq!(NamingConvention::param_name("sample_count"), "sampleCount");
        assert_eq!(NamingConvention::enum_case_name("NOT_FOUND"), "notFound");
    }

    #[test]
    fn test_swift_record_generation() {
        let no_alias_record = Record::new("Point")
            .with_field(RecordField::new("x", Type::Primitive(Primitive::F64)))
            .with_field(RecordField::new("y", Type::Primitive(Primitive::F64)));
        let output = Swift::render_record(&no_alias_record);
        assert!(output.trim().is_empty(), "No extension needed when field names match");

        let aliased_record = Record::new("SensorData")
            .with_field(RecordField::new("sensor_id", Type::Primitive(Primitive::I32)))
            .with_field(RecordField::new("timestamp_ms", Type::Primitive(Primitive::U64)));
        let output = Swift::render_record(&aliased_record);
        assert!(output.contains("extension SensorData"));
        assert!(output.contains("public var sensorId: Int32"));
        assert!(output.contains("get { sensor_id }"));
        assert!(output.contains("public var timestampMs: UInt64"));
        assert!(output.contains("self.init(sensor_id: sensorId, timestamp_ms: timestampMs)"));
    }

    #[test]
    fn test_swift_enum_generation() {
        let module = create_test_module();
        let enumeration = module.find_enum("SensorStatus").unwrap();
        let output = Swift::render_enum(enumeration);

        assert!(output.contains("public enum SensorStatus: Int32"));
        assert!(output.contains("case idle = 0"));
        assert!(output.contains("case active = 1"));
        assert!(output.contains("case error = 2"));
    }

    #[test]
    fn test_enum_detection() {
        let c_style = Enumeration::new("Status")
            .with_variant(Variant::new("ok"))
            .with_variant(Variant::new("error"));

        let data_enum = Enumeration::new("Result")
            .with_variant(Variant::new("success").with_field(RecordField::new("value", Type::Primitive(Primitive::I32))))
            .with_variant(Variant::new("failure"));

        assert!(c_style.is_c_style());
        assert!(!data_enum.is_c_style());
        assert!(data_enum.is_data_enum());
    }

    #[test]
    fn test_swift_class_generation() {
        let module = create_test_module();
        let class = module.find_class("Sensor").unwrap();
        let output = Swift::render_class(class, &module);

        assert!(output.contains("public final class Sensor"));
        assert!(output.contains("let handle: OpaquePointer"));
        assert!(output.contains("func getReading()"));
        assert!(output.contains("func predictNext("));
        assert!(output.contains("async"));
        assert!(output.contains("deinit"));
    }
}
