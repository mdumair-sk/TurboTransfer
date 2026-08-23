use camino::Utf8Path;

fn main() {
    let udl_path = Utf8Path::new("core/src/turbotransfer_core.udl");
    let cdylib_path = if cfg!(target_os = "windows") {
        Utf8Path::new("target/debug/turbotransfer_core.dll")
    } else if cfg!(target_os = "macos") {
        Utf8Path::new("target/debug/libturbotransfer_core.dylib")
    } else {
        Utf8Path::new("target/debug/libturbotransfer_core.so")
    };

    let out_dir = Utf8Path::new("android/app/src/main/java");

    println!("Generating Kotlin bindings using {:?} and {:?}", udl_path, cdylib_path);

    uniffi_bindgen::generate_bindings(
        udl_path,
        None,
        uniffi_bindgen::bindings::KotlinBindingGenerator,
        Some(out_dir),
        Some(cdylib_path),
        Some("turbotransfer_core"),
        false,
    )
    .expect("Failed to generate Kotlin bindings");

    println!("Successfully generated Kotlin bindings!");
}
