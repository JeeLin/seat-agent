fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    let proto_path = format!("{}/../../proto/seat_agent.proto", manifest_dir);
    tonic_build::compile_protos(&proto_path)?;
    Ok(())
}
