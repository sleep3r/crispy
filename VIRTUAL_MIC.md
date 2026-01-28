# Crispy Virtual Microphone (macOS)

The Crispy virtual microphone allows other applications (Zoom, Discord, OBS, etc.) to use your processed audio as an input device.

## Architecture

The virtual microphone system consists of three components:

1. **AudioServerPlugIn** (`macos/virtual-mic/`) - A CoreAudio plugin that creates a virtual input device called "Crispy Microphone"
2. **Shared Memory IPC** (`crates/virtual_mic_ipc/`) - Lock-free ring buffer for audio data exchange
3. **Audio Engine** (`src-tauri/src/audio_engine.rs`) - Captures, processes, and writes audio to the ring buffer

```
┌─────────────────────┐
│  Physical Mic       │
└──────────┬──────────┘
           │
┌──────────▼──────────┐
│  Crispy App         │
│  ┌──────────────┐   │
│  │ Capture      │   │
│  │ ↓            │   │
│  │ Resample     │   │
│  │ ↓            │   │
│  │ Downmix Mono │   │
│  │ ↓            │   │
│  │ DSP/Effects  │   │
│  │ ↓            │   │
│  │ Ring Buffer  │◄──┼─── Shared Memory
│  └──────────────┘   │
└─────────────────────┘
           │
┌──────────▼──────────┐
│  Virtual Mic Plugin │
│  (CoreAudio)        │
└──────────┬──────────┘
           │
┌──────────▼──────────┐
│  Zoom/Discord/OBS   │
│  (reads as input)   │
└─────────────────────┘
```

## Installation

### Prerequisites

- macOS (tested on Apple Silicon, should work on Intel)
- Xcode Command Line Tools: `xcode-select --install`
- Rust toolchain (via rustup)
- Node.js and npm

### Build and Install Plugin

```bash
# Build the plugin
make plugin-build

# Install the plugin (requires sudo for system audio folder)
make plugin-install

# Check installation status
make plugin-status
```

The plugin will be installed to `/Library/Audio/Plug-Ins/HAL/CrispyVirtualMic.driver`.

### Verify Installation

After installation:

1. Open **System Settings → Sound → Input**
2. You should see **"Crispy Microphone"** in the list
3. Or run `make plugin-status` in the terminal

## Usage

### Start Processing

1. Launch the Crispy app
2. Select your physical microphone in **Settings → Audio I/O → Microphone**
3. The app automatically starts writing processed audio to the virtual mic
4. Check the **Virtual Microphone Status** section to confirm it's active

### Use in Other Apps

In Zoom, Discord, OBS, or any other audio app:

1. Open audio settings
2. Select **"Crispy Microphone"** as the input device
3. Audio will now come from Crispy's processed output

## Testing

### Test with Tone Generator

Before using your real microphone, test the virtual mic with a tone generator:

```bash
# Writes a 440Hz sine wave to the virtual mic
make plugin-test
```

While this is running, open QuickTime Player:

1. File → New Audio Recording
2. Click the dropdown next to the record button
3. Select **"Crispy Microphone"**
4. You should see audio levels and hear the tone when recording

Press `Ctrl+C` to stop the tone generator.

### Test with Real Audio

1. Start the Crispy app
2. Select your physical mic
3. Open Audio MIDI Setup (in Applications → Utilities)
4. Find "Crispy Microphone" in the device list
5. Click the volume meter icon - you should see input levels when speaking

## Audio Format

The virtual microphone uses:

- **Sample Rate:** 48,000 Hz
- **Channels:** 1 (mono)
- **Format:** Float32 (linear PCM)
- **Buffer:** 200ms ring buffer (9600 frames)

Input audio is automatically:
- Resampled to 48kHz if needed
- Downmixed to mono if stereo/multichannel
- Converted to Float32

## Troubleshooting

### Plugin not appearing in device list

```bash
# Restart CoreAudio
make plugin-restart

# Or manually:
sudo launchctl kickstart -k system/com.apple.audio.coreaudiod
```

### No audio in virtual mic

1. Check that Crispy app is running
2. Verify physical mic is selected and working (check the meter)
3. Look at Virtual Microphone Status in Settings - should show "Active"
4. Check Console.app for CoreAudio errors

### Underruns / audio dropouts

The status shows underrun/overrun counters. If you see many underruns:
- Your CPU might be overloaded
- The physical mic sample rate might be causing issues
- Try a different physical mic or sample rate

### Permission denied errors

The plugin needs to be installed in a system directory:

```bash
# Use sudo for installation
sudo make plugin-install
```

## Uninstallation

```bash
# Remove the plugin
make plugin-uninstall

# This will:
# 1. Delete /Library/Audio/Plug-Ins/HAL/CrispyVirtualMic.driver
# 2. Restart CoreAudio
# 3. Remove the device from the system
```

## Development

### Building

```bash
# Build just the plugin
cd macos/virtual-mic
make

# Build in release mode
cargo build --release -p crispy_virtual_mic_plugin
```

### Architecture Notes

- **Plugin runs at x86_64** (via Rosetta on Apple Silicon) because the Rust toolchain is x86_64
- CoreAudio runs the plugin in a realtime audio thread - no allocations or locks allowed
- The C shim (`plugin.m`) forwards calls to Rust core (`lib.rs`)
- Shared memory uses POSIX `shm_open` with atomic indices for lock-free reads/writes

### Code Signing (for distribution)

For public release, the plugin must be signed:

```bash
# Sign the plugin bundle
codesign --force --sign "Developer ID Application" \
  target/CrispyVirtualMic.driver

# Verify signature
codesign --verify --verbose target/CrispyVirtualMic.driver
```

And notarized:

```bash
# Create a zip for notarization
ditto -c -k --keepParent target/CrispyVirtualMic.driver CrispyVirtualMic.zip

# Submit for notarization
xcrun notarytool submit CrispyVirtualMic.zip \
  --apple-id "your@email.com" \
  --team-id "TEAMID" \
  --wait

# Staple the notarization ticket
xcrun stapler staple target/CrispyVirtualMic.driver
```

## Technical Details

### Shared Memory Protocol

Location: `/dev/shm/crispy_virtual_mic`

Header (aligned):
- Magic: `0x43525350` ("CRSP")
- Version: 1
- Sample rate: 48000
- Channels: 1
- Format: 0 (Float32)
- Capacity: 9600 frames
- Atomic write_index (u32)
- Atomic read_index (u32)
- Atomic underrun_count (u64)
- Atomic overrun_count (u64)
- Atomic sequence (u64)

Buffer: `capacity * channels * sizeof(f32)` bytes following header

### Performance

- **Latency:** ~40-80ms (configurable via buffer target fill)
- **CPU:** <1% on Apple M1 (audio processing + ring buffer writes)
- **Memory:** ~40KB shared memory + plugin overhead

## References

- [CoreAudio AudioServerPlugIn Documentation](https://developer.apple.com/documentation/coreaudio/audio_hardware_services)
- [CPAL Cross-platform Audio I/O](https://github.com/RustAudio/cpal)
- [Apple's SimpleAudioDriver Example](https://developer.apple.com/library/archive/samplecode/SimpleAudioDriver/)
