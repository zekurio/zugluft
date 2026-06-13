fn main() {
    println!("cargo:rerun-if-changed=zugluft.rc");
    println!("cargo:rerun-if-changed=assets/app-icon.ico");

    #[cfg(windows)]
    embed_resource::compile("zugluft.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("failed to embed Windows resources");
}
