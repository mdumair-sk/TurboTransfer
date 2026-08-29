use camino::Utf8Path;

fn main() {
    let udl_path = Utf8Path::new("core/src/turbotransfer_core.udl");
    let cdylib_path = if Utf8Path::new("target/release/libturbotransfer_core.so").exists() {
        Utf8Path::new("target/release/libturbotransfer_core.so")
    } else if Utf8Path::new("target/release/turbotransfer_core.dll").exists() {
        Utf8Path::new("target/release/turbotransfer_core.dll")
    } else if Utf8Path::new("target/debug/libturbotransfer_core.so").exists() {
        Utf8Path::new("target/debug/libturbotransfer_core.so")
    } else {
        Utf8Path::new("target/debug/turbotransfer_core.dll")
    };

    let out_dir = Utf8Path::new("/data/data/com.termux/files/home/turbotransfer_uniffi_out");
    let _ = std::fs::create_dir_all(out_dir);

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
