//! Captures the target triple so the runtime library search can look in the same
//! `vendor/pdfium/<triple>/` directory that `xtask vendor-pdfium` writes to.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!(
        "cargo:rustc-env=OC_PDF_TARGET_TRIPLE={}",
        std::env::var("TARGET").unwrap_or_default()
    );
}
