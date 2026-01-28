# Makefile for Crispy Virtual Microphone AudioServerPlugIn

PLUGIN_NAME = CrispyVirtualMic
BUNDLE_ID = com.example.crispy.virtualmicrophone
DRIVER_BUNDLE = $(PLUGIN_NAME).driver

# Directories
# Build for x86_64 since Rust toolchain is x86_64 (works on Apple Silicon via Rosetta)
ARCH = x86_64
BUILD_DIR = ../../target
RUST_LIB = $(BUILD_DIR)/release/libcrispy_virtual_mic_core.a
BUNDLE_DIR = $(BUILD_DIR)/$(DRIVER_BUNDLE)
CONTENTS_DIR = $(BUNDLE_DIR)/Contents
MACOS_DIR = $(CONTENTS_DIR)/MacOS
RESOURCES_DIR = $(CONTENTS_DIR)/Resources

# Compiler settings
CC = clang
CFLAGS = -Wall -O2 -arch $(ARCH)
LDFLAGS = -framework CoreAudio -framework CoreFoundation -framework Foundation
LDFLAGS += -bundle -Wl,-exported_symbols_list,exports.txt

.PHONY: all clean install uninstall

all: $(BUNDLE_DIR)

# Build Rust static library (release mode)
$(RUST_LIB):
	@echo "Building Rust core library..."
	cd ../.. && cargo build --release -p crispy_virtual_mic_plugin

# Create bundle structure
$(BUNDLE_DIR): $(RUST_LIB)
	@echo "Creating bundle structure..."
	@mkdir -p $(MACOS_DIR)
	@mkdir -p $(RESOURCES_DIR)
	
	@echo "Compiling plugin..."
	$(CC) $(CFLAGS) -c src/plugin.m -o src/plugin.o
	
	@echo "Linking plugin..."
	$(CC) $(CFLAGS) $(LDFLAGS) src/plugin.o $(RUST_LIB) -o $(MACOS_DIR)/$(PLUGIN_NAME)
	
	@echo "Copying Info.plist..."
	cp Info.plist $(CONTENTS_DIR)/Info.plist
	
	@echo "Plugin bundle created at $(BUNDLE_DIR)"
	@rm -f src/plugin.o

clean:
	@echo "Cleaning plugin build..."
	@rm -rf $(BUNDLE_DIR)
	@rm -f src/plugin.o

install: $(BUNDLE_DIR)
	@echo "Installing plugin to /Library/Audio/Plug-Ins/HAL/..."
	@sudo mkdir -p /Library/Audio/Plug-Ins/HAL
	@sudo rm -rf /Library/Audio/Plug-Ins/HAL/$(DRIVER_BUNDLE)
	@sudo cp -R $(BUNDLE_DIR) /Library/Audio/Plug-Ins/HAL/
	@echo "Plugin installed. Restart CoreAudio:"
	@echo "  sudo launchctl kickstart -k system/com.apple.audio.coreaudiod"

uninstall:
	@echo "Uninstalling plugin..."
	@sudo rm -rf /Library/Audio/Plug-Ins/HAL/$(DRIVER_BUNDLE)
	@echo "Plugin uninstalled. Restart CoreAudio:"
	@echo "  sudo launchctl kickstart -k system/com.apple.audio.coreaudiod"
