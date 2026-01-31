fn main() {
    // Add Swift runtime library paths for screencapturekit on macOS
    #[cfg(target_os = "macos")]
    {
        // Add Xcode toolchain Swift runtime path
        println!("cargo:rustc-link-arg=-Wl,-rpath,/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx");
        // Add system Swift runtime path
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
        // Add executable path for bundled libraries
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");
        // Ensure ScreenCaptureKit framework is linked (fixes x86_64 linker errors)
        println!("cargo:rustc-link-lib=framework=ScreenCaptureKit");
    }
    
    tauri_build::build()
}