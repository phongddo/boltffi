use askama::Template;

#[derive(Template)]
#[template(path = "render_python/module.txt", escape = "none")]
pub struct ModuleTemplate<'a> {
    pub module: &'a super::PythonModule,
}
