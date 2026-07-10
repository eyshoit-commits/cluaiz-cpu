use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR is not set");
    let llama_path = Path::new(&out_dir).join("llama.cpp");

    // ── Phase 1: Clone llama.cpp ─────────────────────────────────────

    if !llama_path.exists() {
        println!("cargo:warning=🔩 Cloning official ggml-org/llama.cpp source...");

        let llama_path_str = llama_path
            .to_str()
            .expect("llama.cpp path contains invalid UTF-8");

        let status = Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                "https://github.com/ggml-org/llama.cpp",
                llama_path_str,
            ])
            .status()
            .expect("Failed to execute git clone");

        if !status.success() {
            panic!("Failed to clone llama.cpp");
        }
    }

    // Rebuild when the relevant upstream sources change.
    println!(
        "cargo:rerun-if-changed={}",
        llama_path.join("CMakeLists.txt").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        llama_path.join("src").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        llama_path.join("ggml").display()
    );

    // ── Phase 2: Configure CPU-only build ────────────────────────────

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    println!("cargo:warning=🖥️  Configuring llama.cpp as CPU-only build...");

    let mut config = cmake::Config::new(&llama_path);

    config
        // Build only the libraries required by the Rust FFI driver.
        .define("LLAMA_BUILD_EXAMPLES", "OFF")
        .define("LLAMA_BUILD_TESTS", "OFF")
        .define("LLAMA_BUILD_SERVER", "OFF")
        .define("LLAMA_BUILD_TOOLS", "OFF")
        .define("LLAMA_BUILD_APP", "OFF")
        .define("LLAMA_BUILD_COMMON", "OFF")
        .define("LLAMA_BUILD_UI", "OFF")
        .define("LLAMA_TOOLS_INSTALL", "OFF")
        .define("LLAMA_TESTS_INSTALL", "OFF")
        .define("LLAMA_OPENSSL", "OFF")
        .define("BUILD_SHARED_LIBS", "OFF")
        // CPU backend only.
        .define("GGML_CPU", "ON")
        .define("GGML_CUDA", "OFF")
        .define("GGML_METAL", "OFF")
        .define("GGML_VULKAN", "OFF")
        .define("GGML_HIPBLAS", "OFF")
        .define("GGML_OPENCL", "OFF")
        .define("GGML_OPENVINO", "OFF")
        .define("GGML_SYCL", "OFF")
        .define("GGML_QNN", "OFF")
        .define("GGML_CANN", "OFF")
        // Disable the irrelevant ccache warning.
        .define("GGML_CCACHE", "OFF")
        // Only relevant on MSVC; harmless on other platforms.
        .define(
            "CMAKE_MSVC_RUNTIME_LIBRARY",
            "MultiThreaded",
        )
        .profile("Release");

    // ── Apple target alignment ───────────────────────────────────────

    if target_os == "ios" {
        let apple_arch = if target_arch == "aarch64" {
            "arm64"
        } else {
            target_arch.as_str()
        };

        config
            .define("CMAKE_SYSTEM_NAME", "iOS")
            .define("CMAKE_OSX_SYSROOT", "iphoneos")
            .define("CMAKE_OSX_ARCHITECTURES", apple_arch)
            .define("CMAKE_OSX_DEPLOYMENT_TARGET", "16.0");
    } else if target_os == "macos" {
        config.define("CMAKE_OSX_DEPLOYMENT_TARGET", "11.0");
    }

    let dst = config.build();

    // ── Phase 3: Locate and link static libraries ────────────────────

    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-search=native={}/lib64", dst.display());

    find_and_link_search_paths(&dst);

    println!("cargo:rustc-link-lib=static=llama");
    println!("cargo:rustc-link-lib=static=ggml");
    println!("cargo:rustc-link-lib=static=ggml-base");
    println!("cargo:rustc-link-lib=static=ggml-cpu");

    // ── Platform system libraries ────────────────────────────────────

    match target_os.as_str() {
        "windows" => {
            println!("cargo:rustc-link-lib=dylib=advapi32");
            println!("cargo:rustc-link-lib=dylib=user32");
            println!("cargo:rustc-link-lib=dylib=ws2_32");
        }

        "macos" => {
            println!("cargo:rustc-link-lib=framework=Foundation");
            println!("cargo:rustc-link-lib=framework=Accelerate");
            println!("cargo:rustc-link-lib=dylib=c++");
        }

        "ios" => {
            println!("cargo:rustc-link-lib=framework=Foundation");
            println!("cargo:rustc-link-lib=framework=Accelerate");
            println!("cargo:rustc-link-lib=dylib=c++");
        }

"linux" => {
    println!("cargo:rustc-link-lib=dylib=stdc++");

    // llama.cpp/ggml-cpu was compiled with GNU OpenMP.
    // Provides GOMP_*, omp_get_thread_num, omp_get_num_threads, etc.
    println!("cargo:rustc-link-lib=dylib=gomp");

    println!("cargo:rustc-link-lib=dylib=pthread");
    println!("cargo:rustc-link-lib=dylib=dl");
    println!("cargo:rustc-link-lib=dylib=m");
}

        "android" => {
            println!("cargo:rustc-link-lib=dylib=c++");
            println!("cargo:rustc-link-lib=dylib=log");
            println!("cargo:rustc-link-lib=dylib=dl");
            println!("cargo:rustc-link-lib=dylib=m");
        }

        _ => {
            println!("cargo:rustc-link-lib=dylib=stdc++");
        }
    }

    println!("cargo:warning=✅ [Llama-Engine] CPU-only CMake build complete.");
}

fn find_and_link_search_paths(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    let mut contains_static_library = false;

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();

        if path.is_dir() {
            find_and_link_search_paths(&path);
            continue;
        }

        let is_static_library = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension == "a" || extension == "lib");

        if is_static_library {
            contains_static_library = true;
        }
    }

    if contains_static_library {
        println!("cargo:rustc-link-search=native={}", dir.display());
    }
}