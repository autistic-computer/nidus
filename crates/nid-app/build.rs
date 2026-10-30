fn main() {
    println!("cargo:rerun-if-changed=nid.rc");
    println!("cargo:rerun-if-changed=../../assets/windows/nid.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("nid.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("the icon resource compiles");
    }
}
