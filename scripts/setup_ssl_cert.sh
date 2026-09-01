#!/bin/bash
# Setup SSL certificate for s3.crispy.fyi using Let's Encrypt

set -e

DOMAIN="s3.crispy.fyi"
EMAIL="sleep3r@icloud.com"
# Keep everything under $HOME so no sudo/root is needed.
CERT_DIR="$HOME/letsencrypt"

echo "🔐 Setting up SSL certificate for $DOMAIN"

# Check if certbot is installed
if ! command -v certbot &> /dev/null; then
    echo "❌ certbot not found. Installing..."
    if [[ "$OSTYPE" == "darwin"* ]]; then
        brew install certbot
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        sudo apt-get update
        sudo apt-get install -y certbot
    else
        echo "❌ Unsupported OS. Please install certbot manually."
        exit 1
    fi
fi

# Use DNS challenge (works for S3/CNAME domains)
echo "📋 Using DNS challenge (manual verification required)"
echo ""
echo "This will generate certificates using DNS challenge."
echo "You'll need to add TXT records to your DNS provider."
echo ""

# Generate certificate using DNS challenge (no sudo: everything under $CERT_DIR)
certbot certonly \
    --manual \
    --preferred-challenges dns \
    --config-dir "$CERT_DIR" \
    --work-dir "$CERT_DIR/work" \
    --logs-dir "$CERT_DIR/logs" \
    --email "$EMAIL" \
    --agree-tos \
    --no-eff-email \
    -d "$DOMAIN"

echo ""
echo "✅ Certificate generated!"
echo ""
echo "📁 Certificate location:"
echo "   Certificate: $CERT_DIR/live/$DOMAIN/fullchain.pem"
echo "   Private Key: $CERT_DIR/live/$DOMAIN/privkey.pem"
echo ""
echo "📋 Next steps:"
echo "   1. Copy certificate and private key from above paths"
echo "   2. Add them to your S3 provider's domain settings"
echo "   3. Update mc alias to use https://$DOMAIN"
