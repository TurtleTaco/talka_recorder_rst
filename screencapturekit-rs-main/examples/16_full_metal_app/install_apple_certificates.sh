#!/bin/bash
# Install Apple's intermediate certificates needed for code signing

echo "📥 Installing Apple Intermediate Certificates..."
echo ""

# Create temp directory
TEMP_DIR=$(mktemp -d)
cd "${TEMP_DIR}"

echo "Downloading Apple certificates..."

# Download Developer ID G1 Intermediate Certificate
curl -O https://www.apple.com/certificateauthority/DeveloperIDG1.cer

# Download Developer ID G2 Intermediate Certificate (newer)
curl -O https://www.apple.com/certificateauthority/DeveloperIDG2.cer

# Download AppleWWDRCA certificate
curl -O https://www.apple.com/certificateauthority/AppleWWDRCA.cer

# Download AppleWWDRCAG2
curl -O https://www.apple.com/certificateauthority/AppleWWDRCAG2.cer

# Download AppleWWDRCAG3
curl -O https://www.apple.com/certificateauthority/AppleWWDRCAG3.cer

echo ""
echo "Installing certificates..."

# Import all certificates into the system keychain
sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain DeveloperIDG1.cer
sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain DeveloperIDG2.cer
sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain AppleWWDRCA.cer
sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain AppleWWDRCAG2.cer
sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain AppleWWDRCAG3.cer

# Clean up
cd -
rm -rf "${TEMP_DIR}"

echo ""
echo "✅ Apple certificates installed!"
echo ""
echo "Now try building again:"
echo "  ./build_and_distribute.sh"

