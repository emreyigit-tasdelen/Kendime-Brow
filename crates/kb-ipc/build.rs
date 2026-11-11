// Compiles proto/ with protox (pure Rust; no protoc install needed) and prost-build.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = "../../proto";
    let files = ["kb/ipc/v1/ipc.proto"];
    for f in files {
        println!("cargo:rerun-if-changed={proto_root}/{f}");
    }
    let fds = protox::compile(files, [proto_root])?;
    prost_build::Config::new().compile_fds(fds)?;
    Ok(())
}
