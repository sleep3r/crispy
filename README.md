# Crispy

**A free, open source, and privacy-focused noise suppression application that works completely offline.**

Crispy is a cross-platform desktop application built with Tauri (Rust + React/TypeScript) that aims to provide real-time noise suppression for your microphone. It processes audio locally to remove background noise without sending your voice to the cloud.

## Why Crispy?

*   **Private**: Your voice stays on your computer. No cloud processing.
*   **Simple**: Clean, black-and-white interface designed for focus.
*   **Open**: Built on open technologies like Tauri and Rust.

## Current Status

🚧 **Early Development**

Current focus:
*   ✅ **Device selection** - Working input/output device enumeration
*   ✅ **Live audio monitoring** - Real-time input level meters
*   ✅ **BlackHole 2 integration** - Required for audio routing on macOS
*   🚧 **Noise suppression** - DSP framework in place, awaiting ML model integration

## Quick Start

### Prerequisites

*   macOS (for BlackHole 2 routing)
*   Node.js (for frontend)
*   Rust (for backend)

### Development

1.  **Install dependencies:**
    ```bash
    make install
    # or: npm install
    ```

2.  **Install BlackHole 2ch (macOS only):**
    ```bash
    # Download and install from:
    # https://existential.audio/blackhole/
    ```
    BlackHole 2ch is required for routing audio to other apps.

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

### Audio Routing

Crispy relies on **BlackHole 2ch** for macOS audio routing. If BlackHole is not installed,
the app will warn you and disable audio routing features.

## Architecture

Crispy is built as a Tauri application with a simple audio pipeline:

*   **Frontend**: React + TypeScript with Tailwind CSS (v4) for the UI
*   **Backend**: Rust using CPAL for audio I/O and custom real-time processing
*   **Audio Engine**: Capture → Process → Meter
*   **Routing**: BlackHole 2ch (system driver)

```
Physical Mic → Crispy App → BlackHole 2ch → Zoom/Discord/etc.
```

## Roadmap

*   [x] UI Scaffold (Handy-inspired)
*   [x] Device Selection UI
*   [x] Real-time Audio Monitoring
*   [x] BlackHole-based routing on macOS
*   [ ] Noise Suppression Model Integration
*   [ ] DSP effects (compression, EQ, etc.)
*   [ ] Windows virtual audio device support

## Documentation

- [Makefile](Makefile) - All available build targets and commands

## Helpful Commands

```bash
make help              # Show all available commands
make dev              # Run in development mode
```

## License

MIT
