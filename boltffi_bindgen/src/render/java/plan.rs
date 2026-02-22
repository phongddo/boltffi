use super::JavaVersion;

#[derive(Debug, Clone)]
pub struct JavaModule {
    pub package_name: String,
    pub class_name: String,
    pub lib_name: String,
    pub java_version: JavaVersion,
    pub prefix: String,
    pub functions: Vec<JavaFunction>,
}

impl JavaModule {
    pub fn package_path(&self) -> String {
        self.package_name.replace('.', "/")
    }
}

#[derive(Debug, Clone)]
pub enum JavaReturnStrategy {
    Void,
    Direct,
    WireDecode { decode_expr: String },
}

impl JavaReturnStrategy {
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Direct)
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

    pub fn native_return_type<'a>(&self, return_type: &'a str) -> &'a str {
        match self {
            Self::Void => "void",
            Self::Direct => return_type,
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
}

impl JavaFunction {
    pub fn native_return_type(&self) -> &str {
        self.strategy.native_return_type(&self.return_type)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JavaParamKind {
    Direct,
    Utf8Bytes,
}

#[derive(Debug, Clone)]
pub struct JavaParam {
    pub name: String,
    pub java_type: String,
    pub native_type: String,
    pub kind: JavaParamKind,
}

impl JavaParam {
    pub fn needs_conversion(&self) -> bool {
        self.kind != JavaParamKind::Direct
    }

    pub fn to_native_expr(&self) -> String {
        match &self.kind {
            JavaParamKind::Direct => self.name.clone(),
            JavaParamKind::Utf8Bytes => format!(
                "{}.getBytes(java.nio.charset.StandardCharsets.UTF_8)",
                self.name
            ),
        }
    }
}
