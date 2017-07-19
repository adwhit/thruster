#[macro_use]
extern crate error_chain;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate openapi;
extern crate handlebars;
extern crate rocket;

pub use errors::*;
pub use std::path;
pub use std::collections::BTreeMap;
pub use std::fs::File;
pub use std::io::{Read, Write};
use openapi::{Spec, Operation, Operations, ParameterOrRef};
use handlebars::Handlebars;
use serde_json::Value as JsonValue;

mod errors {
    error_chain!{
        foreign_links {
            Io(::std::io::Error);
            OpenApi(::openapi::errors::Error);
            Render(::handlebars::RenderError);
            Template(::handlebars::TemplateError);
        }
    }
}

pub fn load<P: AsRef<path::Path>>(path: P) -> Result<Spec> {
    let spec = openapi::from_path(path)?;
    Ok(spec)
}

pub fn generate<P: AsRef<path::Path>>(spec: &Spec, path: P) -> Result<()> {
    let mut gen = File::create("/home/alex/scratch/swaggergen/src/gen.rs")?;
    let mut stub = File::create("/home/alex/scratch/swaggergen/src/stub.rs")?;
    let mut reg = Handlebars::new();
    let mut routes = Vec::new();

    writeln!(gen, "{}", GEN_HEADER)?;
    writeln!(stub, "{}", STUB_HEADER)?;

    reg.register_template_string("route", ROUTE_TEMPLATE)?;
    reg.register_template_string("stub", STUB_TEMPLATE)?;

    for (path, operations) in &spec.paths {
        let operations = transform_operations(operations);
        for (method, op) in &operations {
            routes.push(op.operation_id.as_ref().unwrap().clone());
            let tmpl_args = build_route_args(path, *method, op);

            let rendered = reg.render("route", &tmpl_args)?;
            writeln!(gen, "{}", rendered)?;

            let stubbed = reg.render("stub", &tmpl_args)?;
            writeln!(stub, "{}", stubbed)?;
        }
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

fn build_route_args(route: &str, method: Method, operation: &Operation) ->  JsonValue {
    let parameters = if let Some(ref params) = operation.parameters {
        build_parameters(params)
    } else {
        vec![]
    };
    json!({
        "method": method,
        "route": route,
        // TODO verify that operation_id is valid
        "function": operation.operation_id,
        "parameters": parameters
    })
}

fn transform_operations(operations: &Operations) -> BTreeMap<Method, &Operation> {
    use Method::*;
    let mut map = BTreeMap::new();
    if let Some(ref op) = operations.get {
        map.insert(Get, op);
    }
    if let Some(ref op) = operations.post {
        map.insert(Post, op);
    }
    if let Some(ref op) = operations.put {
        map.insert(Put, op);
    }
    if let Some(ref op) = operations.patch {
        map.insert(Patch, op);
    }
    if let Some(ref op) = operations.delete {
        map.insert(Delete, op);
    }
    map
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
    {{#each parameters as |parameter|}}
    {{parameter.name}}: {{parameter.type}}
    {{/each}}
) -> io::Result<{{result_type}}> {
    {{function}}()
}"#;

const STUB_TEMPLATE: &str = r#"
pub fn {{function}}() -> io::Result<{{result_type}}> {
    unimplemented!()
}"#;

#[derive(Serialize, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

fn build_parameters(parameters: &Vec<ParameterOrRef>) -> Vec<JsonValue> {
    use ParameterOrRef::*;
    let mut res = Vec::new();
    for param in parameters {
        let json = match *param {
            Parameter{ref name, ..} => json!({
                "name": name,
                "type": "fake",
            }),
            Ref{ ref ref_path } => json!({
                "name": "fake",
                "type": "fake",
            })
        };
        res.push(json);
    }
    res
}
