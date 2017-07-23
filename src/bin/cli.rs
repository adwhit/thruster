extern crate thruster;

use thruster::*;

#[macro_use]
extern crate error_chain;


fn run() -> Result<()> {
    let src_path = "/home/alex/scratch/anywhere/src";
    let spec = OpenApi::from_file("example_apis/petstore.yaml")?;
    // bootstrap(spec, dir_path)?;
    generate_sources(&spec, src_path)?;
    Ok(())
}

quick_main!(run);
