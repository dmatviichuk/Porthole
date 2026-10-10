fn main() {
    println!("cargo:rerun-if-changed=../../assets/porthole.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("../../assets/porthole.ico")
            .set("ProductName", "Porthole")
            .set("FileDescription", "Desktop client for Kubernetes")
            .set("OriginalFilename", "porthole.exe")
            .set("LegalCopyright", "Copyright © 2026 Dmytro Matviichuk")
            .compile()
            .expect("compile the Windows application icon and version metadata");
    }
}
