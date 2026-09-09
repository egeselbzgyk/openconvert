//! Captures the triple `xtask` itself was built for, which is the host triple that
//! `vendor-pdfium` needs in order to pick the right PDFium asset.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!(
        "cargo:rustc-env=XTASK_HOST_TRIPLE={}",
        std::env::var("TARGET").unwrap_or_default()
    );
}
