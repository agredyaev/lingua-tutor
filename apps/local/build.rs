//! Embed the prebuilt web UI into the self-contained CLI binary.

use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let dist = manifest.join("../web/dist");
    println!("cargo:rerun-if-changed={}", dist.display());

    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(&dist) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && let Some(name) = path.file_name().and_then(|value| value.to_str())
            {
                files.push((name.to_owned(), path));
            }
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));

    const MAX_WASM_BYTES: u64 = 3 * 1024 * 1024;
    let mut generated = String::from("const EMBEDDED_ASSETS: &[(&str, &[u8])] = &[\n");
    for (name, path) in files {
        if path.extension().and_then(|value| value.to_str()) == Some("wasm") {
            let bytes = fs::metadata(&path)?.len();
            if bytes > MAX_WASM_BYTES {
                return Err(format!(
                    "embedded WASM exceeds 3 MB raw budget: {} bytes ({})",
                    bytes,
                    path.display()
                )
                .into());
            }
        }
        generated.push_str(&format!(
            "    ({name:?}, include_bytes!({path:?})),\n",
            name = name,
            path = path.to_string_lossy(),
        ));
    }
    generated.push_str("];\n");

    let out = PathBuf::from(env::var("OUT_DIR")?).join("embedded_assets.rs");
    fs::write(out, generated)?;
    Ok(())
}
