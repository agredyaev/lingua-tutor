//! Embedded web assets for the self-contained CLI.

include!(concat!(env!("OUT_DIR"), "/embedded_assets.rs"));

/// Materialize embedded UI assets into a project-local state directory.
pub fn write_to(directory: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    for (name, bytes) in EMBEDDED_ASSETS {
        std::fs::write(directory.join(name), bytes).map_err(|error| error.to_string())?;
    }
    if !directory.join("index.html").is_file() {
        return Err("embedded web assets are missing index.html".to_owned());
    }
    Ok(())
}
