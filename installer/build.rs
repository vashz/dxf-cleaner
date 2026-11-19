fn main() {
    println!("cargo:rerun-if-env-changed=DXF_APP_EXE");
    println!("cargo:rerun-if-env-changed=DXF_APP_VERSION");
    println!("cargo:rerun-if-changed=../assets/icon.ico");
    println!("cargo:rerun-if-changed=setup.manifest");
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../assets/icon.ico")
        .set_manifest_file("setup.manifest")
        .set("ProductName", "DXF Converter Setup")
        .set("FileDescription", "DXF Converter Setup");
    if let Err(e) = res.compile() {
        panic!("failed to embed Windows resources: {e}");
    }
}
