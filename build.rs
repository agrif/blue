use std::env;
use std::error::Error;
use std::path::Path;

fn main() -> Result<(), Box<dyn Error>> {
    let cargo_raw = env::var("CARGO")?;
    let cargo = Path::new(&cargo_raw);
    let manifest_dir_raw = env::var("CARGO_MANIFEST_DIR")?;
    let manifest_dir = Path::new(&manifest_dir_raw);
    // FIXME this might be overridden by CARGO_TARGET_DIR
    let target_dir = env::current_dir()?.join(manifest_dir.join("target"));
    let release = env::var("PROFILE")? == "release";
    let llvm_tools = llvm_tools::LlvmTools::new()
        .expect("LLVM tools not found, use: rustup component add llvm-tools-preview");
    let objcopy = llvm_tools
        .tool(&llvm_tools::exe("llvm-objcopy"))
        .expect("llvm-objcopy not found");

    // rerun if libs changed
    println!("cargo:rerun-if-changed={}", manifest_dir.join("real").display());

    // careful: stage1 is size sensitive, so always release
    build(
        &cargo,
        &objcopy,
        &manifest_dir.join("loader-stage1"),
        &target_dir,
        "i586-unknown-none-code16",
        true,
        "blue-loader-stage1",
    )?;

    // rust debug symbols are *too big* for 16-bit code
    build(
        &cargo,
        &objcopy,
        &manifest_dir.join("loader-stage2"),
        &target_dir,
        "i586-unknown-none-code16",
        true,
        "blue-loader-stage2",
    )?;

    build(
        &cargo,
        &objcopy,
        &manifest_dir.join("loader-stage3"),
        &target_dir,
        "x86_64-unknown-none",
        release,
        "blue-loader-stage3",
    )?;

    Ok(())
}

fn build(
    cargo: &Path,
    objcopy: &Path,
    source: &Path,
    root_target_dir: &Path,
    triple: &str,
    release: bool,
    outputname: &str,
) -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed={}", source.display());

    let name = source
        .file_stem()
        .ok_or("can't find file stem")?
        .to_str()
        .ok_or("path is not valid utf-8")?;
    let target_dir = root_target_dir.join(name);

    let mut cmd = std::process::Command::new(cargo);
    cmd.current_dir(source)
        .arg("build")
        .arg(format!("--target-dir={}", target_dir.display()));
    if release {
        cmd.arg("--release");
    }

    let status = cmd.status()?;
    if !status.success() {
        Err("build failed")?;
    }

    let outputdir = target_dir
        .join(triple)
        .join(if release { "release" } else { "debug" });
    let output = outputdir.join(outputname);
    let binary = outputdir.join(outputname.to_string() + ".bin");

    let mut objcopy_cmd = std::process::Command::new(objcopy);
    objcopy_cmd.arg("-O").arg("binary").arg(output).arg(&binary);
    let objcopy_status = objcopy_cmd.status()?;
    if !objcopy_status.success() {
        Err("objcopy failed")?;
    }

    let varname = format!("BLUE_{}", name.replace("-", "_").to_uppercase());
    println!("cargo:rustc-env={}={}", varname, binary.display());

    Ok(())
}
