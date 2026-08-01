fn main() {
    println!("cargo:rustc-link-search=native=/usr/local/lib");
    println!("cargo:rustc-link-lib=dylib=playfusion_projectm_native");
    println!("cargo:rerun-if-changed=build.rs");
}
