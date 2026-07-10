fn main() {
    tauri_build::build();

    // 下载 ONNX Runtime 动态库（仅 Windows，配合 ort load-dynamic 特性）
    // 避免链接预编译静态库时 MSVC 版本不兼容（如 VS2019 缺少 __std_minmax_element_4）
    #[cfg(target_os = "windows")]
    {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let lib_dir = std::path::Path::new(&manifest_dir).join("lib");
        let dll_path = lib_dir.join("onnxruntime.dll");

        std::fs::create_dir_all(&lib_dir).ok();

        if !dll_path.exists() {
            println!("cargo:warning=ONNX Runtime DLL 未找到，正在下载...");
            let url = "https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-win-x64-1.20.1.zip";
            let zip_path = lib_dir.join("onnxruntime.zip");

            // 使用 PowerShell 下载
            let download_status = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!("Invoke-WebRequest -Uri '{}' -OutFile '{}'", url, zip_path.display()),
                ])
                .status();

            match download_status {
                Ok(s) if s.success() => {
                    // 解压并提取 DLL
                    let extract_status = std::process::Command::new("powershell")
                        .args([
                            "-NoProfile",
                            "-Command",
                            &format!(
                                "Expand-Archive -Path '{}' -DestinationPath '{}' -Force; Move-Item -Force '{}\\onnxruntime-win-x64-1.20.1\\lib\\onnxruntime.dll' '{}'",
                                zip_path.display(),
                                lib_dir.display(),
                                lib_dir.display(),
                                dll_path.display()
                            ),
                        ])
                        .status();
                    let _ = std::fs::remove_file(&zip_path);

                    if extract_status.map(|s| s.success()).unwrap_or(false) {
                        println!("cargo:warning=ONNX Runtime DLL 下载完成");
                    } else {
                        println!("cargo:warning=ONNX Runtime DLL 解压失败，请手动下载 onnxruntime.dll 放至 {}", lib_dir.display());
                    }
                }
                _ => {
                    println!("cargo:warning=ONNX Runtime DLL 下载失败，请手动下载 onnxruntime.dll 放至 {}", lib_dir.display());
                    println!("cargo:warning=下载地址: {}", url);
                }
            }
        }

        if dll_path.exists() {
            println!("cargo:rustc-env=ORT_DYLIB_PATH={}", dll_path.display());
        }

        println!("cargo:rerun-if-changed=build.rs");
    }
}
