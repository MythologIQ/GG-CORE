// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Build script for Veritas SDR
//!
//! Generates C header file when `ffi` feature is enabled.

fn main() {
    // Only run cbindgen when ffi feature is enabled
    #[cfg(feature = "ffi")]
    {
        if let Err(e) = generate_c_header() {
            println!("cargo:warning=Failed to generate C header: {}", e);
        }
    }
}

#[cfg(feature = "ffi")]
fn generate_c_header() -> Result<(), Box<dyn std::error::Error>> {
    use std::env;
    use std::path::PathBuf;

    let crate_dir = env::var("CARGO_MANIFEST_DIR")?;
    let output_dir = PathBuf::from(&crate_dir).join("include");

    // Ensure include directory exists
    std::fs::create_dir_all(&output_dir)?;

    let output_file = output_dir.join("veritas_sdr.h");
    let config_path = PathBuf::from(&crate_dir).join("cbindgen.toml");

    if config_path.exists() {
        let config = cbindgen::Config::from_file(&config_path)?;

        if let Ok(bindings) = cbindgen::Builder::new()
            .with_crate(&crate_dir)
            .with_config(config)
            .generate()
        {
            bindings.write_to_file(&output_file);
            println!("cargo:rerun-if-changed=cbindgen.toml");
            println!("cargo:rerun-if-changed=src/ffi/");
        } else {
            println!("cargo:warning=cbindgen could not generate bindings");
        }
    }

    Ok(())
}
