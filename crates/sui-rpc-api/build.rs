use std::fs;
use std::path::PathBuf;

fn main() {
    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let sui_proto_dir = crate_dir.join("proto");
    let proto_file = sui_proto_dir.join("sui/rpc/v2beta2/event_service.proto");
    let out_dir = crate_dir.join("src/proto/generated");

    println!("cargo:rerun-if-changed={}", proto_file.display());
    println!("cargo:rerun-if-changed={}", sui_proto_dir.display());

    fs::create_dir_all(&out_dir).expect("create proto out dir");

    let config = tonic_build::configure();

    config
        .build_client(true)
        .build_server(true)
        .out_dir(&out_dir)
        .compile_protos(&[proto_file], &[sui_proto_dir])
        .expect("compile event_service.proto");
}
