use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src/boot.S");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let boot_object = out_dir.join("boot.o");
    let status = Command::new("as")
        .arg("--64")
        .arg("-o")
        .arg(&boot_object)
        .arg("src/boot.S")
        .status()
        .expect("failed to execute the GNU assembler");

    assert!(status.success(), "failed to assemble src/boot.S");
    println!("cargo:rustc-link-arg={}", boot_object.display());
}
