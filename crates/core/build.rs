//! Hashes the vendored 0.2 schema for the hello: the engine refuses a client built against another one.

use sha2::{Digest, Sha256};

const SCHEMA: &str = "schema/0.2.schema.json";

fn main() {
    println!("cargo:rerun-if-changed={SCHEMA}");
    let bytes = std::fs::read(SCHEMA).unwrap_or_else(|e| panic!("cannot read {SCHEMA}: {e}"));
    let digest = Sha256::digest(bytes);
    let prefix = digest[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    println!("cargo:rustc-env=BN_SCHEMA_HASH=sha256:{prefix}");
}
