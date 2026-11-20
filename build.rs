fn main() {
    slint_build::compile("ui/app.slint").expect("compile ui/app.slint");
    println!("cargo:rerun-if-changed=ui/app.slint");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set_manifest_file("assets/app.manifest");
        res.set("FileDescription", "Clip Translator");
        res.set("ProductName", "Clip Translator");
        res.set("LegalCopyright", "Copyright (c) 2026 Jun");
        res.set("OriginalFilename", "clip-translator.exe");
        res.compile().expect("embed the Windows icon and manifest");
    }
}
