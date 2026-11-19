//! Protobuf code generation, run only with the `pb` feature (its build dependencies are optional).

#[cfg(feature = "pb")]
#[path = "build_pb.rs"]
mod pb;

#[cfg(feature = "pb")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    pb::main()
}

#[cfg(not(feature = "pb"))]
fn main() {}
