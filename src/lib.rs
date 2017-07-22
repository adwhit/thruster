#[macro_use]
extern crate error_chain;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate handlebars;
extern crate rocket;
extern crate openapi3;
extern crate regex;
#[macro_use]
extern crate derive_new;

pub use errors::*;
pub use std::path;
pub use std::collections::BTreeMap;
pub use std::fs::File;
pub use std::io::{Read, Write};
use handlebars::Handlebars;
use serde_json::Value as JsonValue;
pub use openapi3::OpenApi;

mod errors {
    error_chain!{
        foreign_links {
            Io(::std::io::Error);
            Render(::handlebars::RenderError);
            Template(::handlebars::TemplateError);
            OpenApi(::openapi3::Error);
        }
    }
}

mod process;

pub fn generate<P: AsRef<path::Path>>(spec: &OpenApi, path: P) -> Result<()> {

    let entrypoints = process::extract_entrypoints(spec);
    let mut routes = Vec::new();

    let mut reg = Handlebars::new();
    reg.register_template_string("route", ROUTE_TEMPLATE)?;
    reg.register_template_string("stub", STUB_TEMPLATE)?;

    let mut gen = File::create("/home/alex/scratch/swaggergen/src/gen.rs")?;
    let mut stub = File::create("/home/alex/scratch/swaggergen/src/stub.rs")?;
    writeln!(gen, "{}", GEN_HEADER)?;
    writeln!(stub, "{}", STUB_HEADER)?;

    for entry in entrypoints {
        let tmpl_args = entry.build_template_args();
        println!("{:#?}", tmpl_args);
        routes.push(entry.route);

        let rendered = reg.render("route", &tmpl_args)?;
        writeln!(gen, "{}", rendered)?;

        let stubbed = reg.render("stub", &tmpl_args)?;
        writeln!(stub, "{}", stubbed)?;
    }


    {
        let swagger_args = json!({
            "method": "get",
            "route": "/swagger",
            "function": "swagger"
        });
        routes.push("swagger".into());
        let swaggered = reg.render("route", &swagger_args)?;
        writeln!(gen, "{}", swaggered)?;
    }

    reg.register_template_string("launch", LAUNCH_TEMPLATE)?;
    let launch = reg.render("launch",
                            &json!({"routes": routes}))?;
    writeln!(gen, "{}", launch)?;

    Ok(())
}

const GEN_HEADER: &str = "
use stub::*;
use std::io;
use rocket;
";

const STUB_HEADER: &str = "
use std::io;
";

const LAUNCH_TEMPLATE: &str = r#"
fn launch() -> Result<rocket::Rocket> {
    rocket::ignite().mount("/", routes![
        {{#each routes as |r|~}}
        _{{r}},
        {{/each~}}
    ])
}"#;

const ROUTE_TEMPLATE: &str = r#"
#[{{method}}("{{route}}")]
fn _{{function}}(
    {{#each args as |arg|~}}
    {{arg.name}}: {{arg.type}},
    {{/each~}}
) -> io::Result<{{result_type}}> {
    {{function}}()
}"#;

const STUB_TEMPLATE: &str = r#"
pub fn {{function}}() -> io::Result<{{result_type}}> {
    unimplemented!()
}"#;
