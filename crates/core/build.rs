//! Hashes the vendored 0.2 schema for the hello: the engine refuses a client built against another one.
//! Compiles the client's own catalogues `po/<locale>.po` for `localise::Catalogue::client`.

use sha2::{Digest, Sha256};
use std::path::Path;

const SCHEMA: &str = "schema/0.2.schema.json";
const PO: &str = "po";

fn main() {
    println!("cargo:rerun-if-changed={SCHEMA}");
    let bytes = std::fs::read(SCHEMA).unwrap_or_else(|e| panic!("cannot read {SCHEMA}: {e}"));
    let digest = Sha256::digest(bytes);
    let prefix = digest[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    println!("cargo:rustc-env=BN_SCHEMA_HASH=sha256:{prefix}");

    println!("cargo:rerun-if-changed={PO}");
    let out = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    let mut languages = std::fs::read_dir(PO)
        .unwrap_or_else(|e| panic!("cannot read {PO}: {e}"))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "po"))
        .collect::<Vec<_>>();
    languages.sort();
    let table = languages
        .iter()
        .map(|po| {
            let locale = po.file_stem().unwrap().to_string_lossy();
            let mo = Path::new(&out).join(format!("{locale}.mo"));
            polib::mo_file::compile_from_po(po, &mo)
                .unwrap_or_else(|e| panic!("cannot compile {}: {e}", po.display()));
            format!(
                "({locale:?}, include_bytes!({:?})),",
                mo.display().to_string()
            )
        })
        .collect::<String>();
    std::fs::write(Path::new(&out).join("catalogues.rs"), format!("&[{table}]"))
        .unwrap_or_else(|e| panic!("cannot write the catalogue table: {e}"));
}
