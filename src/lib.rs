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
use std::process::Command;
pub use std::io::{Read, Write};
use handlebars::Handlebars;
pub use openapi3::OpenApi;
use templates::*;

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

pub mod process;
pub mod templates;

struct Config {
    dir_path: String,
    gen: String,
    stub: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dir_path: "/tmp/thruster_generated".into(),
            gen: "gen".into(),
            stub: "stub".into(),
        }
    }
}

pub fn generate_function_stubs<W: Write>(mut writer: W, spec: &OpenApi) -> Result<()> {
    let mut entrypoints = process::extract_entrypoints(spec);
    let swagger = process::Entrypoint::swagger_entrypoint();
    entrypoints.push(swagger);

    let mut reg = Handlebars::new();
    reg.register_escape_fn(handlebars::no_escape);
    reg.register_template_string("stub", STUB_TEMPLATE)?;
    writeln!(writer, "{}", STUB_HEADER)?;

    for entry in entrypoints {
        let tmpl_args = entry.build_template_args();
        let stubbed = reg.render("stub", &tmpl_args)?;
        writeln!(writer, "{}", stubbed)?;
    }

    Ok(())
}

pub fn generate_server_endpoints<W: Write>(mut writer: W, spec: &OpenApi) -> Result<()> {
    let mut entrypoints = process::extract_entrypoints(spec);
    let swagger = process::Entrypoint::swagger_entrypoint();
    entrypoints.push(swagger);

    let mut routes = Vec::new();
    let mut reg = Handlebars::new();
    reg.register_escape_fn(handlebars::no_escape);
    reg.register_template_string("route", ROUTE_TEMPLATE)?;
    writeln!(writer, "{}", GEN_HEADER)?;

    for entry in entrypoints {
        let tmpl_args = entry.build_template_args();
        routes.push(entry.operation_id);

        let rendered = reg.render("route", &tmpl_args)?;
        writeln!(writer, "{}", rendered)?;
    }

    reg.register_template_string("launch", LAUNCH_TEMPLATE)?;
    let launch = reg.render("launch", &json!({ "routes": routes }))?;
    writeln!(writer, "{}", launch)?;

    Ok(())
}

pub fn generate_main<W: Write>(mut writer: W) -> Result<()> {
    let mut reg = Handlebars::new();
    reg.register_escape_fn(handlebars::no_escape);
    reg.register_template_string("main", MAIN_TEMPLATE)?;
    let main = reg.render("main", &json!({ "gen": "gen", "stub": "stub" }))?;
    writeln!(writer, "{}", main)?;
    Ok(())
}

fn cargo_command(args: &[&str]) -> Result<()> {
    let mut child = Command::new("cargo").args(args).spawn()?;
    let ecode = child.wait()?;
    if !ecode.success() {
        bail!("Failed to execute Cargo command: {:?}", args)
    }
    Ok(())
}

fn cargo_new(dir_path: &str) -> Result<()> {
    cargo_command(&["new", "--bin", dir_path])
}

fn cargo_format(dir_path: &str) -> Result<()> {
    cargo_command(&["fmt"])
}

fn cargo_check(dir_path: &str) -> Result<()> {
    cargo_command(&["check"])
}

pub fn bootstrap<P: AsRef<path::Path>>(api_path: P, dir_path: &str) -> Result<()> {
    let api = OpenApi::from_file(api_path)?;
    cargo_new(dir_path)?;

    let gen_name = "gen";
    let stub_name = "stub";

    let path = path::Path::new(dir_path);
    let srcpath = path.join("src");
    let gen_path = srcpath.join(format!("{}.rs", gen_name));
    let stub_path = srcpath.join(format!("{}.rs", stub_name));
    let main_path = srcpath.join("main.rs");

    let gen_file = File::create(gen_path)?;
    generate_server_endpoints(gen_file, &api)?;

    let stub_file = File::create(stub_path)?;
    generate_function_stubs(stub_file, &api)?;

    let main_file = File::create(main_path)?;
    generate_main(main_file)?;

    cargo_format(dir_path)?;
    cargo_check(dir_path)?;

    Ok(())
}
