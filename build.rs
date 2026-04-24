use std::env;
use std::path::PathBuf;

fn build_cmake_tool(src: &str, bin: &str, bin_dir: &PathBuf) {
    println!("cargo:rerun-if-changed=tools/{src}");

    let dst = cmake::Config::new(format!("tools/{src}"))
        .profile("Release")
        .build();

    std::fs::copy(dst.join("bin").join(bin), bin_dir.join(bin))
        .unwrap_or_else(|e| panic!("failed to copy {bin} to bin/: {e}"));
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let bin_dir = manifest_dir.join("bin");
    std::fs::create_dir_all(&bin_dir).expect("failed to create bin/");

    build_cmake_tool("level9", "level9", &bin_dir);
    build_cmake_tool("Magnetic", "magnetic", &bin_dir);
    build_cmake_tool("frotz", "dfrotz", &bin_dir);
}
