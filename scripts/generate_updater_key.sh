#!/bin/bash
# Generate signing key pair for Tauri updater

set -e

KEY_PATH="$HOME/.tauri/crispy-updater.key"

echo "🔑 Generating Tauri updater signing key pair"
echo ""

# Check if tauri CLI is installed
if ! command -v tauri &> /dev/null; then
    echo "❌ Tauri CLI not found. Installing..."
    cargo install tauri-cli --version "^2.0.0"
fi

# Generate key
echo "📝 Generating key to: $KEY_PATH"
tauri signer generate -w "$KEY_PATH"

echo ""
echo "✅ Key pair generated!"
echo ""
echo "📋 Next steps:"
echo "   1. Copy the PUBLIC KEY shown above"
echo "   2. Add it to src-tauri/tauri.conf.json in 'plugins.updater.pubkey'"
echo "   3. Add PRIVATE KEY to GitHub Secrets:"
echo "      - Go to: https://github.com/sleep3r/crispy/settings/secrets/actions"
echo "      - Create secret: TAURI_SIGNING_PRIVATE_KEY"
echo "      - Paste content of: $KEY_PATH"
echo ""
echo "🔐 IMPORTANT: Keep $KEY_PATH secret!"
echo "   Do NOT commit it to git!"
