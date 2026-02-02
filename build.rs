use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    if std::env::var_os("CARGO_FEATURE_PROTOBUF").is_none() {
        return Ok(());
    }

    println!("cargo:rerun-if-changed=proto/test.proto");

    prost_build::compile_protos(&["proto/test.proto"], &["proto"])?;
    Ok(())
}
