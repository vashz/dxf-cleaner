#[path = "src/version.rs"]
mod version;

fn main() {
    println!("cargo:rerun-if-env-changed=DXF_APP_EXE");
    println!("cargo:rerun-if-changed=src/version.rs");
    println!("cargo:rerun-if-env-changed=DXF_APP_VERSION");
    println!("cargo:rerun-if-changed=../assets/icon.ico");
    println!("cargo:rerun-if-changed=setup.manifest");

    // The setup exe carries the version of the app it installs (not this crate's version).
    let version = std::env::var("DXF_APP_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let parts = version::version_parts(&version);
    let packed = (parts[0] << 48) | (parts[1] << 32) | (parts[2] << 16) | parts[3];

    let mut res = winresource::WindowsResource::new();
    res.set_icon("../assets/icon.ico")
        .set_manifest_file("setup.manifest")
        .set("ProductName", "DXF Converter Setup")
        .set("FileDescription", "DXF Converter Setup")
        .set("FileVersion", &version)
        .set("ProductVersion", &version)
        .set_version_info(winresource::VersionInfo::FILEVERSION, packed)
        .set_version_info(winresource::VersionInfo::PRODUCTVERSION, packed);
    if let Err(e) = res.compile() {
        panic!("failed to embed Windows resources: {e}");
    }
}
