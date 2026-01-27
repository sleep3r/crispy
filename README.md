# Crispy

**A free, open source, and privacy-focused noise suppression application that works completely offline.**

Crispy is a cross-platform desktop application built with Tauri (Rust + React/TypeScript) that aims to provide real-time noise suppression for your microphone. It processes audio locally to remove background noise without sending your voice to the cloud.

## Why Crispy?

*   **Private**: Your voice stays on your computer. No cloud processing.
*   **Simple**: Clean, black-and-white interface designed for focus.
*   **Open**: Built on open technologies like Tauri and Rust.

## Current Status

🚧 **Early Development**

Crispy is currently in the initial scaffolding phase. The user interface is in place, featuring:
*   Microphone and Output device selection.
*   Model selection framework (currently a "Dummy Model").
*   Tauri + React + TypeScript foundation.

## Quick Start

### Prerequisites

*   Node.js (for frontend)
*   Rust (for backend)

### Development

1.  **Install dependencies:**
    ```bash
    npm install
    ```

2.  **Run in development mode:**
    ```bash
    npm run tauri dev
    ```

3.  **Build for production:**
    ```bash
    npm run tauri build
    ```

## Architecture

Crispy is built as a Tauri application combining:

*   **Frontend**: React + TypeScript with Tailwind CSS (v4) for the UI.
*   **Backend**: Rust for system integration and audio processing (planned).

## Roadmap

*   [x] UI Scaffold (Handy-inspired)
*   [x] Device Selection UI
*   [ ] Real-time Audio Processing Pipeline
*   [ ] Noise Suppression Model Integration
*   [ ] Virtual Audio Device implementation

## License

MIT
