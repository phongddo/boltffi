use askama::Template;

use super::plan::{JavaEnum, JavaModule, JavaRecord};

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
