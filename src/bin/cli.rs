extern crate thruster;

use thruster::*;

#[macro_use]
extern crate error_chain;

fn run() -> Result<()> {
    let swagger = load("swagger/petstore.yaml")?;
    generate(&swagger, "src/gen.rs")?;
    Ok(())
}

quick_main!(run);
