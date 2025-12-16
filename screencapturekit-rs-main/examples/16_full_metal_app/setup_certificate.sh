#!/bin/bash
# Helper script to set up code signing certificate

echo "🔐 Code Signing Certificate Setup"
echo "=================================="
echo ""

# Check current status
echo "📋 Checking current certificate status..."
CERT_COUNT=$(security find-identity -v -p codesigning | grep "Developer ID Application" | wc -l | tr -d ' ')

if [ "$CERT_COUNT" -gt 0 ]; then
    echo "✅ Found $CERT_COUNT Developer ID Application certificate(s):"
    security find-identity -v -p codesigning | grep "Developer ID Application"
    echo ""
    echo "You're all set! Run ./build_and_distribute.sh to build."
    exit 0
fi

echo "❌ No Developer ID Application certificate found"
echo ""
echo "To get a certificate, follow these steps:"
echo ""
echo "1️⃣  Go to Apple Developer Portal:"
echo "   https://developer.apple.com/account/resources/certificates/list"
echo ""
echo "2️⃣  Click the ➕ button to create a new certificate"
echo ""
echo "3️⃣  Select: 'Developer ID Application'"
echo "   (NOT 'Apple Development' or 'iOS Distribution')"
echo ""
echo "4️⃣  Follow the prompts to:"
echo "   - Create a Certificate Signing Request (CSR) from Keychain Access"
echo "   - Upload the CSR"
echo "   - Download the certificate (.cer file)"
echo ""
echo "5️⃣  Double-click the downloaded certificate to install it in Keychain"
echo ""
echo "6️⃣  Run this script again to verify: ./setup_certificate.sh"
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "📖 Detailed Instructions:"
echo ""
echo "STEP A: Create Certificate Signing Request (CSR)"
echo "  1. Open 'Keychain Access' app (in /Applications/Utilities/)"
echo "  2. Menu: Keychain Access → Certificate Assistant → Request a Certificate from a Certificate Authority"
echo "  3. Enter your email address"
echo "  4. Common Name: Your name"
echo "  5. Select 'Saved to disk' and 'Let me specify key pair information'"
echo "  6. Click Continue"
echo "  7. Save as 'CertificateSigningRequest.certSigningRequest'"
echo "  8. Key Size: 2048 bits, Algorithm: RSA"
echo "  9. Click Continue, then Done"
echo ""
echo "STEP B: Create Certificate on Apple Developer"
echo "  1. Go to: https://developer.apple.com/account/resources/certificates/add"
echo "  2. Select: 'Developer ID Application' (under 'Software')"
echo "  3. Click Continue"
echo "  4. Upload your CertificateSigningRequest.certSigningRequest file"
echo "  5. Click Continue"
echo "  6. Download the certificate (.cer file)"
echo ""
echo "STEP C: Install Certificate"
echo "  1. Double-click the downloaded .cer file"
echo "  2. It will open Keychain Access and install"
echo "  3. Make sure it's in the 'login' keychain (or 'System')"
echo ""
echo "STEP D: Verify"
echo "  Run: ./setup_certificate.sh"
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "💡 Need help? Check the DISTRIBUTION_GUIDE.md for more details"
echo ""

# Check if they have Apple Developer account
echo ""
read -p "Do you have an Apple Developer account? (y/n) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo ""
    echo "You need an Apple Developer account to distribute macOS apps."
    echo "Sign up at: https://developer.apple.com/programs/"
    echo "Cost: \$99/year"
    exit 1
fi

echo ""
echo "Great! Open this URL in your browser to get started:"
echo "https://developer.apple.com/account/resources/certificates/add"
echo ""

