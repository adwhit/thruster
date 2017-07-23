pub(crate) const GEN_HEADER: &str = "
// *** This file was generated by thruster ***

use stub::*;
use types::*;
use std::io;
use rocket;
use rocket_contrib::Json;
";

pub(crate) const ROUTE_TEMPLATE: &str = r#"
#[{{method}}("{{route}}")]
fn _{{function}}(
    {{#each args as |arg|~}}
    {{arg.name}}: {{arg.type}},
    {{/each~}}
) -> Result<Json<{{result_type}}>, ()> {
    {{function}}().map(Json)
}"#;

pub(crate) const LAUNCH_TEMPLATE: &str = r#"
pub fn mount_api(rocket: rocket::Rocket) -> rocket::Rocket {
    rocket.mount("/", routes![
        {{#each routes as |r|~}}
        _{{r}},
        {{/each~}}
    ])
}"#;

pub(crate) const STUB_HEADER: &str = "
// *** This file was generated by thruster ***

use std::io;
use types::*;
";

pub(crate) const STUB_TEMPLATE: &str = r#"
pub fn {{function}}() -> Result<{{result_type}}, ()> {
    unimplemented!()
}"#;

pub(crate) const TYPES_HEADER: &str = r#"
// *** This file was generated by thruster ***
"#;

pub(crate) const MAIN_TEMPLATE: &str = r#"
// *** This file was generated by thruster ***

#![feature(plugin, custom_derive)]
#![plugin(rocket_codegen)]

extern crate rocket;
extern crate rocket_contrib;
#[macro_use]
extern crate serde_derive;
extern crate serde;

mod gen;
mod stub;
mod types;

fn main() {
    let rocket = rocket::Rocket::ignite();
    let rocket = gen::mount_api(rocket);
    println!("{}", rocket.launch());
}"#;
