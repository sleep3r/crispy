# Crispy

**A free, open source, and privacy-focused noise suppression application that works completely offline.**

Crispy is a cross-platform desktop application built with Tauri (Rust + React/TypeScript) that aims to provide real-time noise suppression for your microphone. It processes audio locally to remove background noise without sending your voice to the cloud.

## Why Crispy?

*   **Private**: Your voice stays on your computer. No cloud processing.
*   **Simple**: Clean, black-and-white interface designed for focus.
*   **Open**: Built on open technologies like Tauri and Rust.

## Current Status

🚧 **Early Development** - Virtual Microphone Implemented!

Crispy now includes a working virtual microphone for macOS that other apps can use as an input device:
*   ✅ **Virtual Microphone** - CoreAudio AudioServerPlugIn that creates "Crispy Microphone" device
*   ✅ **Real-time Audio Pipeline** - Captures, resamples, and processes audio with shared-memory IPC
*   ✅ **Microphone and Output device selection** - Working device enumeration and selection
*   ✅ **Live audio monitoring** - Real-time level meters and monitoring
*   🚧 **Noise suppression** - DSP framework in place, awaiting ML model integration

## Quick Start

### Prerequisites

*   macOS (for virtual microphone support)
*   Node.js (for frontend)
*   Rust (for backend)
*   Xcode Command Line Tools: `xcode-select --install`

### Development

1.  **Install dependencies:**
    ```bash
    make install
    # or: npm install
    ```

2.  **Install virtual microphone plugin (macOS only):**
    ```bash
    make plugin-install
    ```
    This creates the "Crispy Microphone" virtual device that other apps can use.

3.  **Run in development mode:**
    ```bash
    make dev
    # or: npm run tauri dev
    ```

4.  **Build for production:**
    ```bash
    make build
    # or: npm run tauri build
    ```

### Using the Virtual Microphone

Once installed, "Crispy Microphone" will appear in:
- System Settings → Sound → Input
- Zoom, Discord, OBS, etc. audio input selection

See [VIRTUAL_MIC.md](VIRTUAL_MIC.md) for detailed documentation.

## Architecture

Crispy is built as a Tauri application with a multi-component audio pipeline:

*   **Frontend**: React + TypeScript with Tailwind CSS (v4) for the UI
*   **Backend**: Rust using CPAL for audio I/O and custom real-time processing
*   **Virtual Mic Plugin**: CoreAudio AudioServerPlugIn (Rust + C shim) with lock-free shared memory IPC
*   **Audio Engine**: Capture → Resample → Downmix → Process → Ring Buffer → Virtual Device

```
Physical Mic → Crispy App → Shared Memory → Virtual Mic Plugin → Zoom/Discord/etc.
```

## Roadmap

*   [x] UI Scaffold (Handy-inspired)
*   [x] Device Selection UI
*   [x] Real-time Audio Processing Pipeline
*   [x] Virtual Audio Device implementation (macOS)
*   [x] Shared-memory IPC with lock-free ring buffer
*   [x] Automatic resampling and format conversion
*   [ ] Noise Suppression Model Integration
*   [ ] DSP effects (compression, EQ, etc.)
*   [ ] Windows virtual audio device support

## Documentation

- [VIRTUAL_MIC.md](VIRTUAL_MIC.md) - Complete virtual microphone documentation
- [Makefile](Makefile) - All available build targets and commands

## Helpful Commands

```bash
make help              # Show all available commands
make plugin-install    # Install virtual microphone plugin
make plugin-status     # Check if plugin is installed
make plugin-test       # Test with tone generator
make dev              # Run in development mode
```

## License

MIT
