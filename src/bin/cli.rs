extern crate thruster;

use thruster::*;

#[macro_use]
extern crate error_chain;


fn run() -> Result<()> {
    let dir_path = "/this/could/be/rotterdam/anywhere";
    let api = "example_apis/petstore-expanded.yaml";
    bootstrap(api, dir_path)?;
    Ok(())
}

quick_main!(run);
