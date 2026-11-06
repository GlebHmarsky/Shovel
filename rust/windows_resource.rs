//! Shared native executable identity; no C# assembly or runtime is embedded.
pub fn compile(executable: &str, description: &str) {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest =
        include_str!("windows.manifest").replace("@IDENTITY@", executable.trim_end_matches(".exe"));
    // A manifest file preserves the XML declaration at byte zero. Inline RC
    // strings insert whitespace before it in winresource 0.1.30.
    let manifest_path =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo output directory"))
            .join("windows.manifest");
    std::fs::write(&manifest_path, manifest).expect("Windows manifest write failed");
    let version = std::env::var("CARGO_PKG_VERSION").expect("Cargo supplies a package version");
    let mut resource = winresource::WindowsResource::new();
    resource
        .set("ProductName", "ShovelRust")
        .set("FileDescription", description)
        .set("OriginalFilename", executable)
        .set("InternalName", executable.trim_end_matches(".exe"))
        .set("FileVersion", &format!("{version}.0"))
        .set("ProductVersion", &version)
        .set("CompanyName", "Shovel contributors")
        .set_icon("../../src/GitGui.App/Assets/Shovel.ico")
        .set_manifest_file(manifest_path.to_str().expect("Windows SDK manifest path"))
        .compile()
        .expect("Windows SDK resource compilation failed");
    println!("cargo:rerun-if-changed=../windows_resource.rs");
    println!("cargo:rerun-if-changed=../windows.manifest");
    println!("cargo:rerun-if-changed=../../src/GitGui.App/Assets/Shovel.ico");
}
