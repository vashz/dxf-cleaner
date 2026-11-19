// Embeds the application icon and version information into the Windows executables.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("ProductName", "DXF Converter")
            .set("FileDescription", "DXF Converter - clean DXF files for cutting")
            .set("InternalName", "dxf-converter");
        if let Err(e) = res.compile() {
            panic!("failed to embed Windows resources: {e}");
        }
    }
}
