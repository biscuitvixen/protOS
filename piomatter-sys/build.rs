//! Compiles the C shim over Piomatter's header-only core and piolib.
//!
//! The Piomatter headers define non-inline functions, so exactly one
//! translation unit may include them: the shim. piolib registers its
//! chip driver through a linker section that nothing references by
//! symbol, so its archive is linked whole and the entry is compiled
//! through a wrapper that marks the section retained (see csrc).

fn main() {
    let vendor = std::path::Path::new("vendor");
    println!("cargo:rerun-if-changed=csrc");
    println!("cargo:rerun-if-changed=vendor");
    cc::Build::new()
        .cpp(true)
        .std("c++20")
        .file("csrc/shim.cpp")
        .include(vendor)
        .include(vendor.join("piolib/include"))
        .flag_if_supported("-pthread")
        .warnings(false)
        .opt_level(2)
        .compile("piomatter_shim");
    cc::Build::new()
        .std("c11")
        .file(vendor.join("piolib/piolib.c"))
        .file("csrc/pio_rp1_retained.c")
        .include(vendor.join("piolib"))
        .include(vendor.join("piolib/include"))
        .define("_GNU_SOURCE", None)
        .warnings(false)
        .opt_level(2)
        .link_lib_modifier("+whole-archive")
        .compile("piolib");
    println!("cargo:rustc-link-lib=pthread");
}
