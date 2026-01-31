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

        // Link required Apple frameworks for ScreenCaptureKit
        println!("cargo:rustc-link-lib=framework=ScreenCaptureKit");
        println!("cargo:rustc-link-lib=framework=CoreMedia");
        println!("cargo:rustc-link-lib=framework=CoreVideo");
        println!("cargo:rustc-link-lib=framework=CoreAudio");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=AudioToolbox");
        println!("cargo:rustc-link-lib=framework=Accelerate");
    }
    
    tauri_build::build()
}
