//! Protobuf code generation, run only with the `pb` feature (its build dependencies are optional).

#[cfg(feature = "protobuf")]
#[path = "build_pb.rs"]
mod pb;

#[cfg(feature = "protobuf")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    pb::main()
}

#[cfg(not(feature = "protobuf"))]
fn main() {}
