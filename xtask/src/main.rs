use std::{
    env,
    path::{Path, PathBuf},
    process::{self, Command},
    fs,
};
use serde::Deserialize;

#[macro_use]
extern crate clap;

const DEFAULT_TARGET: &'static str = "riscv64imac-unknown-none-elf";

#[derive(Debug)]
struct XtaskEnv {
    kernel_package_path: PathBuf,
    kernel_package_name: String,
    kernel_binary_name: String,
    compile_mode: CompileMode,
}

#[derive(Debug)]
enum CompileMode {
    Debug,
    Release
}

fn main() {    
    let matches = clap_app!(xtask =>
        (version: crate_version!())
        (author: crate_authors!())
        (about: crate_description!())
        (@subcommand make =>
            (about: "Build project")
            (@arg release: --release "Build artifacts in release mode, with optimizations")
        )
        (@subcommand asm =>
            (about: "View asm code for project")
        )
        (@subcommand size =>
            (about: "View size for project")
        )
        (@subcommand qemu =>
            (about: "Run QEMU")
            (@arg release: --release "Build artifacts in release mode, with optimizations")
            (@arg app: "Choose the apps to be bundled")
        )
    ).get_matches();
    let kernel_package_path = project_root().join("kernels").join(&default_kernel_path());
    let kernel_package_name = read_package_name(&kernel_package_path);
    let kernel_binary_name = format!("{}.bin", kernel_package_name);
    let mut xtask_env = XtaskEnv {
        kernel_package_path,
        kernel_package_name,
        kernel_binary_name,
        compile_mode: CompileMode::Debug,
    };
    println!("xtask: package {}, mode: {:?}", xtask_env.kernel_package_name, xtask_env.compile_mode);
    if let Some(matches) = matches.subcommand_matches("make") {
        if matches.is_present("release") {
            xtask_env.compile_mode = CompileMode::Release;
        }
        xtask_build_kernel(&xtask_env);
        xtask_binary_kernel(&xtask_env);
        // xtask_build_apps(&xtask_env); // todo: multiple apps
    } else if let Some(matches) = matches.subcommand_matches("qemu") {
        if matches.is_present("release") {
            xtask_env.compile_mode = CompileMode::Release;
        }
        let chosen_app = "hello-world"; // todo: 目前是写死的
        if let Some(app_matches) = matches.values_of("app") {
            for app_name in app_matches {
                println!("xtask: building app {}", app_name);
                xtask_build_app(&xtask_env, app_name);
                xtask_binary_app(&xtask_env, app_name);
            }
        }
        xtask_build_kernel(&xtask_env);
        xtask_binary_kernel(&xtask_env);
        xtask_qemu(&xtask_env, chosen_app);
    } else if let Some(_matches) = matches.subcommand_matches("asm") {
        xtask_build_kernel(&xtask_env);
        xtask_asm_kernel(&xtask_env);
    } else if let Some(_matches) = matches.subcommand_matches("size") {
        xtask_build_kernel(&xtask_env);
        xtask_size_kernel(&xtask_env);
    } else {
        println!("Use `cargo qemu` to run, `cargo xtask --help` for help")
    }
}

fn xtask_build_kernel(xtask_env: &XtaskEnv) {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut command = Command::new(cargo);
    command.current_dir(&xtask_env.kernel_package_path);
    command.arg("build");
    match xtask_env.compile_mode {
        CompileMode::Debug => {},
        CompileMode::Release => { command.arg("--release"); },
    }
    command.args(&["--package", &xtask_env.kernel_package_name]);
    command.args(&["--target", DEFAULT_TARGET]);
    let status = command
        .status().unwrap();
    if !status.success() {
        println!("cargo build failed");
        process::exit(1);
    }
}

fn xtask_build_app(xtask_env: &XtaskEnv, app_name: &str) {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut command = Command::new(cargo);
    command.current_dir(project_root().join("apps").join(app_name));
    command.arg("build");
    match xtask_env.compile_mode {
        CompileMode::Debug => {},
        CompileMode::Release => { command.arg("--release"); },
    }
    command.args(&["--package", app_name]);
    command.args(&["--target", DEFAULT_TARGET]);
    let status = command
        .status().unwrap();
    if !status.success() {
        println!("cargo build failed");
        process::exit(1);
    }
}

fn xtask_binary_app(xtask_env: &XtaskEnv, app_name: &str) {
    let objcopy = "rust-objcopy";
    let status = Command::new(objcopy)
        .current_dir(dist_dir(xtask_env))
        .arg(app_name)
        .arg("--binary-architecture=riscv64")
        .arg("--strip-all")
        .args(&["-O", "binary", &format!("{}.bin", app_name)])
        .status().unwrap();

    if !status.success() {
        println!("objcopy binary failed");
        process::exit(1);
    }
}

fn xtask_asm_kernel(xtask_env: &XtaskEnv) {
    // @{{objdump}} -D {{test-kernel-elf}} | less
    let objdump = "riscv64-unknown-elf-objdump";
    Command::new(objdump)
        .current_dir(dist_dir(xtask_env))
        .arg("-d")
        .arg(&xtask_env.kernel_package_name)
        .status().unwrap();
}

fn xtask_size_kernel(xtask_env: &XtaskEnv) {
    // @{{size}} -A -x {{test-kernel-elf}} 
    let size = "rust-size";
    Command::new(size)
        .current_dir(dist_dir(xtask_env))
        .arg("-A")
        .arg("-x")
        .arg(&xtask_env.kernel_package_name)
        .status().unwrap();
}

fn xtask_binary_kernel(xtask_env: &XtaskEnv) {
    /*
    objdump := "riscv64-unknown-elf-objdump"
objcopy := "rust-objcopy --binary-architecture=riscv64"

build: firmware
    @{{objcopy}} {{test-kernel-elf}} --strip-all -O binary {{test-kernel-bin}}
 */
    let objcopy = "rust-objcopy";
    let status = Command::new(objcopy)
        .current_dir(dist_dir(xtask_env))
        .arg(&xtask_env.kernel_package_name)
        .arg("--binary-architecture=riscv64")
        .arg("--strip-all")
        .args(&["-O", "binary", &xtask_env.kernel_binary_name])
        .status().unwrap();

    if !status.success() {
        println!("objcopy binary failed");
        process::exit(1);
    }
}

fn xtask_qemu(xtask_env: &XtaskEnv, one_app: &str) {
    /*
    qemu: build
    @qemu-system-riscv64 \
            -machine virt \
            -nographic \
            -bios none \
            -device loader,file={{rustsbi-bin}},addr=0x80000000 \
            -device loader,file={{test-kernel-bin}},addr=0x80200000 \
            -smp threads={{threads}}
    */
    let status = Command::new("qemu-system-riscv64")
        .current_dir(dist_dir(xtask_env))
        .args(&["-machine", "virt"])
        .args(&["-bios", "../../../bootloader/rustsbi-qemu.bin"])
        .arg("-nographic")
        .args(&["-kernel", &xtask_env.kernel_binary_name])
        .args(&["-device", &format!("loader,file={}.bin,addr=0x80400000", one_app)])
        .status().unwrap();
    
    if !status.success() {
        println!("qemu failed");
        process::exit(1);
    }
}

fn project_root() -> PathBuf {
    Path::new(&env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .unwrap()
        .to_path_buf()
}

fn dist_dir(xtask_env: &XtaskEnv) -> PathBuf {
    let mut path_buf = project_root().join("target").join(DEFAULT_TARGET);
    path_buf = match xtask_env.compile_mode {
        CompileMode::Debug => path_buf.join("debug"),
        CompileMode::Release => path_buf.join("release"),
    };
    path_buf
}

fn read_package_name(path: &Path) -> String {
    let path = path.join("Cargo.toml");
    let buf = fs::read_to_string(path).expect("read package cargo toml file");
    let cfg: PackageToml = toml::from_str(&buf).expect("deserialize package cargo toml");
    cfg.package.name
}

#[derive(Debug, Deserialize, PartialEq)]
struct PackageToml {
    package: Package,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Package {
    name: String,
}

fn default_kernel_path() -> String {
    let workspace_toml = project_root().join("Cargo.toml");
    let buf = fs::read_to_string(workspace_toml).expect("read workspace cargo toml file");
    let cfg: WorkspaceToml = toml::from_str(&buf).expect("deserialize workspace cargo toml");
    cfg.workspace.metadata.xtask.default_kernel_path
}

#[derive(Debug, Deserialize, PartialEq)]
struct WorkspaceToml {
    workspace: Workspace,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Workspace {
    metadata: WorkspaceMetadata
}

#[derive(Debug, Deserialize, PartialEq)]
struct WorkspaceMetadata {
    xtask: XtaskMetadata,
}

#[derive(Debug, Deserialize, PartialEq)]
struct XtaskMetadata {
    #[serde(rename = "default-kernel-path")]
    default_kernel_path: String,
}
