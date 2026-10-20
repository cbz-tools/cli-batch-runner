use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=LICENSE");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR"));
    let icon_path = manifest_dir.join("assets").join("icon.ico");
    println!("cargo:rerun-if-changed={}", icon_path.display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir
            .join("assets")
            .join("icon_source.png")
            .display()
    );
    let license_path = manifest_dir.join("LICENSE");
    let license = fs::read_to_string(&license_path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", license_path.display()));
    let copyright = license
        .lines()
        .find(|line| line.starts_with("Copyright "))
        .unwrap_or_else(|| panic!("LICENSE has no formal copyright line"));
    let version = format!(
        "{}.{}.{}.0",
        env::var("CARGO_PKG_VERSION_MAJOR").expect("Cargo sets package version major"),
        env::var("CARGO_PKG_VERSION_MINOR").expect("Cargo sets package version minor"),
        env::var("CARGO_PKG_VERSION_PATCH").expect("Cargo sets package version patch"),
    );

    let mut resource = winres::WindowsResource::new();
    resource
        .set_icon(icon_path.to_string_lossy().as_ref())
        .set("FileDescription", "CLI Batch Runner")
        .set("InternalName", "cli-batch-runner")
        .set("OriginalFilename", "cli-batch-runner.exe")
        .set("ProductName", "CLI Batch Runner")
        .set("FileVersion", &version)
        .set("ProductVersion", &version)
        .set("LegalCopyright", copyright);
    resource
        .compile()
        .expect("could not compile Windows version resources");
}
