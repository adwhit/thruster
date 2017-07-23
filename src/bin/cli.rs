extern crate thruster;

use thruster::*;

#[macro_use]
extern crate error_chain;


fn run() -> Result<()> {
    let dir_path = "/home/alex/scratch/anywhere";
    let api = "example_apis/petstore.yaml";
    bootstrap(api, dir_path)?;
    Ok(())
}

quick_main!(run);
