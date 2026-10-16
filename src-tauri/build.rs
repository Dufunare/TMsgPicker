use std::{
    env, fs,
    path::{Path, PathBuf},
};

const RUNTIME_DLLS: &[&str] = &[
    "tdjson.dll",
    "libssl-3-x64.dll",
    "libcrypto-3-x64.dll",
];

fn main() {
    println!("cargo:rerun-if-env-changed=TMSGPICKER_TDLIB_DIR");
    if env::var_os("CARGO_FEATURE_DESKTOP").is_none() {
        return;
    }
    tauri_build::build();

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target_os != "windows" {
        panic!("TDLib bring-up currently supports Windows only");
    }

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    assert_eq!(target_arch, "x86_64");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // src-tauri/ -> repository root
    let repo_root = manifest_dir.parent().unwrap();

    // Existing local TDLib build artifacts
    let native_dir = env::var_os("TMSGPICKER_TDLIB_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let direct = repo_root.join("Release");
            if direct.join("tdjson.lib").is_file() {
                direct
            } else {
                direct.join("Release")
            }
        });

    // vcpkg's zlib runtime name has changed across releases. Never rename a DLL:
    // Windows PE import names must be satisfied by the actual built DLL.
    let zlib_dll = ["z.dll", "zlib1.dll"]
        .into_iter()
        .find(|name| native_dir.join(name).is_file())
        .unwrap_or_else(|| panic!(
            "Missing zlib runtime in {} (z.dll or zlib1.dll)", native_dir.display()
        ));
    let runtime_dlls = RUNTIME_DLLS.iter().copied()
        .chain(std::iter::once(zlib_dll))
        .collect::<Vec<_>>();

    // Check required native files
    for filename in runtime_dlls.iter().copied().chain(["tdjson.lib"]) {
        let path = native_dir.join(filename);
        assert!(
            path.is_file(),
            "Missing native file: {}. Set TMSGPICKER_TDLIB_DIR to the Windows x64 TDLib directory (or use --no-default-features for offline core tests).",
            path.display()
        );
        println!("cargo:rerun-if-changed={}", path.display());
    }

    // MSVC linker uses tdjson.lib
    println!("cargo:rustc-link-search=native={}", native_dir.display());

    // OUT_DIR normally:
    // target/debug/build/<package-hash>/out
    // or target/release/build/<package-hash>/out
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("Cannot locate Cargo profile directory");

    // Copy runtime DLLs beside the executable
    for filename in runtime_dlls {
        copy_if_changed(&native_dir.join(filename), &profile_dir.join(filename));
        let deps = profile_dir.join("deps");
        fs::create_dir_all(&deps).expect("Cannot create DLL staging directory");
        copy_if_changed(&native_dir.join(filename), &deps.join(filename));
    }
}

fn copy_if_changed(source: &Path, destination: &Path) {
    let needs_copy = match fs::metadata(destination) {
        Ok(existing) => {
            let original = fs::metadata(source).unwrap();
            existing.len() != original.len() || existing.modified().ok() != original.modified().ok()
        }
        Err(_) => true,
    };

    if needs_copy {
        fs::copy(source, destination)
            .unwrap_or_else(|e| panic!("Cannot copy {}: {e}", source.display()));
    }
}
