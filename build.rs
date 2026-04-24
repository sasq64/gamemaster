use std::env;
use std::path::PathBuf;

fn build_cmake_tool(src: &str, bin: &str, bin_dir: &PathBuf, build_root: &PathBuf) {
    println!("cargo:rerun-if-changed=tools/{src}");

    let out_dir = build_root.join(src);
    std::fs::create_dir_all(&out_dir)
        .unwrap_or_else(|e| panic!("failed to create build dir for {src}: {e}"));

    let dst = cmake::Config::new(format!("tools/{src}"))
        .profile("Release")
        .out_dir(&out_dir)
        .define("CMAKE_EXPORT_COMPILE_COMMANDS", "ON")
        .build();

    std::fs::copy(dst.join("bin").join(bin), bin_dir.join(bin))
        .unwrap_or_else(|e| panic!("failed to copy {bin} to bin/: {e}"));
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let bin_dir = manifest_dir.join("bin");
    let build_root = manifest_dir.join("build");
    std::fs::create_dir_all(&bin_dir).expect("failed to create bin/");
    std::fs::create_dir_all(&build_root).expect("failed to create build/");

    build_cmake_tool("level9", "level9", &bin_dir, &build_root);
    build_cmake_tool("Magnetic", "magnetic", &bin_dir, &build_root);
    build_cmake_tool("frotz", "dfrotz", &bin_dir, &build_root);
}
