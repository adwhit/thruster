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
extern crate tempdir;
extern crate inflector;
#[macro_use]
extern crate derive_new;

use std::path::Path;
use std::fs::File;
use std::process::Command;
use std::io::Write;
use handlebars::Handlebars;
pub use openapi3::OpenApi;
use tempdir::TempDir;

pub use errors::*;
use process::Entrypoint;

mod errors {
    error_chain!{
        foreign_links {
            Io(::std::io::Error);
            Render(::handlebars::RenderError);
            Template(::handlebars::TemplateError);
            TemplateFile(::handlebars::TemplateFileError);
            OpenApi(::openapi3::Error); // TODO goes in links?
        }
    }
}

pub mod process;

const HEADER: &str = r#"
// *** This file was generated by thruster ***
"#;

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

pub fn generate_server_endpoints<W: Write>(
    mut writer: W,
    handlebars: &Handlebars,
    entrypoints: &Vec<Entrypoint>,
) -> Result<()> {
    let tmpl_args = json!({
        "entrypoints": entrypoints
            .iter()
            .map(|entry| entry.build_template_args())
            .collect::<Vec<_>>()
    });
    let rendered = handlebars.render("gen", &tmpl_args)?;
    writeln!(writer, "{}", rendered)?;
    Ok(())
}

pub fn generate_function_stubs<W: Write>(
    mut writer: W,
    handlebars: &Handlebars,
    entrypoints: &Vec<Entrypoint>,
) -> Result<()> {
    let tmpl_args = json!({
        "entrypoints": entrypoints
            .iter()
            .map(|entry| entry.build_template_args())
            .collect::<Vec<_>>()
    });
    let rendered = handlebars.render("stub", &tmpl_args)?;
    writeln!(writer, "{}", rendered)?;
    Ok(())
}

pub fn generate_types<W: Write>(
    mut writer: W,
    handlebars: &Handlebars,
    spec: &OpenApi) -> Result<()> {
    use openapi3::objects::CodeGen;
    writeln!(writer, "{}", HEADER)?;
    spec.components
        .as_ref()
        .and_then(|components| components.schemas.as_ref())
        .map(|schemas| {
            schemas
                .iter()
                .map(|(name, schema)| {
                    println!("Generating type: {}", name);
                    let code = schema.generate_code(name)?;
                    writeln!(writer, "{}", code)?;
                    Ok(())
                })
                .collect::<Result<Vec<()>>>()
                .map(|_| ())
        })
        .unwrap_or(Ok(()))
}

pub fn generate_main<W: Write>(mut writer: W, handlebars: &Handlebars) -> Result<()> {
    let main = handlebars.render(
        "main",
        &json!({"gen": "gen", "stub": "stub"}))?;
    writeln!(writer, "{}", main)?;
    Ok(())
}

pub fn generate_sources<P: AsRef<Path>>(spec: &OpenApi, src_path: P) -> Result<()> {
    let src_path: &Path = src_path.as_ref();

    let gen_name = "gen";
    let stub_name = "stub";
    let types_name = "types";

    let gen_path = src_path.join(format!("{}.rs", gen_name));
    let stub_path = src_path.join(format!("{}.rs", stub_name));
    let types_path = src_path.join(format!("{}.rs", types_name));
    let main_path = src_path.join("main.rs");

    let mut entrypoints = process::extract_entrypoints(spec);
    let swagger = process::Entrypoint::swagger_entrypoint();
    entrypoints.push(swagger);

    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(handlebars::no_escape);
    // TODO grab templates from user input
    handlebars.register_template_file("gen", "templates/gen.hbs")?;
    handlebars.register_template_file("stub", "templates/stub.hbs")?;
    handlebars.register_template_file("main", "templates/main.hbs")?;

    println!("Generating server endpoints");
    let gen_file = File::create(gen_path)?;
    generate_server_endpoints(gen_file, &handlebars, &entrypoints)?;

    println!("Generating stub functions");
    let stub_file = File::create(stub_path)?;
    generate_function_stubs(stub_file, &handlebars, &entrypoints)?;

    println!("Generating types");
    let types_file = File::create(types_path)?;
    generate_types(types_file, &handlebars, &spec)?;

    println!("Generating main");
    let main_file = File::create(main_path)?;
    generate_main(main_file, &handlebars)?;

    Ok(())
}

pub fn bootstrap<P: AsRef<Path>>(spec_path: P, dir_path: P) -> Result<()> {
    // TODO assumes cargo, cargo fmt and cargo add are installed

    let spec = OpenApi::from_file(spec_path)?;

    let tmp_dir = TempDir::new("thruster-bootstrap")?;
    println!("Created temporary dir: {}", tmp_dir.path().to_string_lossy());

    let crate_name: &str = dir_path
        .as_ref()
        .file_name()
        .ok_or("Could not extract crate name from path".into())
        .and_then(|s| {
            s.to_str()
                .ok_or(ErrorKind::from("Crate name must be valid UTF-8"))
        })?;
    cargo_new(tmp_dir.path(), crate_name)?;

    let crate_path = tmp_dir.path().join(crate_name);
    let srcpath = crate_path.join("src");

    generate_sources(&spec, &srcpath)?;

    cargo_fmt(&crate_path)?;
    cargo_add(&crate_path)?;
    cargo_check(&crate_path)?;

    // TODO don't move if already exists
    let mut child = Command::new("mv")
        .current_dir(tmp_dir.path())
        .args(&[crate_name, dir_path.as_ref().to_str().unwrap()])
        .spawn()?;
    let ecode = child.wait()?;
    if !ecode.success() {
        bail!("Failed to execute 'mv' command")
    }

    Ok(())
}

fn cargo_command<P: AsRef<Path>>(dir_path: P, args: &[&str]) -> Result<()> {
    let mut child = Command::new("cargo")
        .current_dir(dir_path)
        .args(args)
        .spawn()?;
    let ecode = child.wait()?;
    if !ecode.success() {
        bail!("Failed to execute Cargo command: {:?}", args)
    }
    Ok(())
}

fn cargo_new<P: AsRef<Path>>(dir_path: P, crate_name: &str) -> Result<()> {
    cargo_command(dir_path, &["new", "--bin", crate_name])
}

fn cargo_fmt<P: AsRef<Path>>(dir_path: P) -> Result<()> {
    cargo_command(dir_path, &["fmt"])
}

fn cargo_check<P: AsRef<Path>>(dir_path: P) -> Result<()> {
    cargo_command(dir_path, &["check"])
}

fn cargo_add<P: AsRef<Path>>(dir_path: P) -> Result<()> {
    cargo_command(
        dir_path,
        &["add", "rocket", "rocket_codegen", "serde", "serde_derive"],
    )
}
