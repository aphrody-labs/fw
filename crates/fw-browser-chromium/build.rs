//! Makes the binary and the addon find libcef at run time: rpath to their own directory (the packaged layout)
//! and to the CEF directory the build resolved through cef-dll-sys (the in-tree layout).

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rustc-link-arg=-Wl,-rpath,$ORIGIN");
    if let Ok(dir) = std::env::var("DEP_CEF_DLL_WRAPPER_CEF_DIR") {
        println!("cargo::rustc-link-arg=-Wl,-rpath,{dir}");
        println!("cargo::rustc-env=browser_chromium_BUILD_CEF_DIR={dir}");
    }
    #[cfg(feature = "node")]
    napi_build::setup();
}
