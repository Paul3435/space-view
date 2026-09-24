fn main() {
    println!("cargo:rerun-if-changed=assets/disktree.rc");
    println!("cargo:rerun-if-changed=assets/disktree.exe.manifest");
    println!("cargo:rerun-if-changed=assets/disktree.ico");

    // Embed icon, manifest (DPI awareness, longPathAware, asInvoker) and
    // version info into the Windows exe. Cross-compiling uses llvm-rc; see
    // scripts/build-windows.sh.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for("assets/disktree.rc", ["disktree"], embed_resource::NONE)
            .manifest_required()
            .expect("failed to compile Windows resources (is llvm-rc / rc.exe installed?)");
    }
}
