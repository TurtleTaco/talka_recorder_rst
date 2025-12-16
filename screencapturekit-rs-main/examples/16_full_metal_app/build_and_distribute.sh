#!/bin/bash
# Build script for distributing the 16_full_metal_app on macOS
#
# Usage:
#   ./build_and_distribute.sh              # Normal build
#   ./build_and_distribute.sh --clean      # Clean build from scratch

set -e

APP_NAME="TalkaCapturePro"
VERSION="1.0.0"
BINARY_NAME="16_full_metal_app"

echo "🔨 Building ${APP_NAME} for macOS distribution..."

# Parse command line arguments
CLEAN_BUILD=false
for arg in "$@"; do
    case $arg in
        --clean|-c)
            CLEAN_BUILD=true
            shift
            ;;
    esac
done

# Optional: Clean previous builds
if [[ "${CLEAN_BUILD}" == "true" ]]; then
    echo "🧹 Cleaning previous builds..."
    cargo clean --release 2>/dev/null || echo "⚠️  Skipping clean (not critical)"
    rm -rf dist 2>/dev/null || true
else
    echo "⏭️  Skipping clean (use --clean flag to clean first)"
fi

# Build for current architecture (release mode)
echo "🏗️  Building for current architecture (release mode)..."
cd /Users/x/Documents/git/talka_recorder_rst/screencapturekit-rs-main
cargo build --example ${BINARY_NAME} --features macos_15_0 --release

# The binary will be at:
BINARY_PATH="target/release/examples/${BINARY_NAME}"

echo "✅ Build complete!"
echo "📦 Binary location: ${BINARY_PATH}"
echo "📏 Binary size: $(du -h ${BINARY_PATH} | cut -f1)"

# Optional: Build universal binary (Intel + Apple Silicon)
echo ""
read -p "🔄 Build universal binary (Intel + Apple Silicon)? (y/n) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "🏗️  Building for x86_64 (Intel)..."
    cargo build --example ${BINARY_NAME} --features macos_15_0 --release --target x86_64-apple-darwin
    
    echo "🏗️  Building for aarch64 (Apple Silicon)..."
    cargo build --example ${BINARY_NAME} --features macos_15_0 --release --target aarch64-apple-darwin
    
    echo "🔗 Creating universal binary..."
    lipo -create \
        target/x86_64-apple-darwin/release/examples/${BINARY_NAME} \
        target/aarch64-apple-darwin/release/examples/${BINARY_NAME} \
        -output target/release/examples/${BINARY_NAME}-universal
    
    BINARY_PATH="target/release/examples/${BINARY_NAME}-universal"
    echo "✅ Universal binary created: ${BINARY_PATH}"
    echo "📏 Universal binary size: $(du -h ${BINARY_PATH} | cut -f1)"
fi

# Create distribution package as proper .app bundle
echo ""
echo "📦 Creating distribution package as .app bundle..."
DIST_DIR="dist/${APP_NAME}-${VERSION}"

# Clean the dist directory for this version to avoid old files
echo "🧹 Cleaning distribution directory..."
rm -rf "${DIST_DIR}"

APP_BUNDLE="${DIST_DIR}/${APP_NAME}.app"
mkdir -p "${APP_BUNDLE}/Contents/MacOS"
mkdir -p "${APP_BUNDLE}/Contents/Resources"

# Copy the actual binary to Resources (not MacOS, since we'll use a launcher)
cp "${BINARY_PATH}" "${APP_BUNDLE}/Contents/Resources/${BINARY_NAME}"

# Strip debug symbols to reduce size
echo "✂️  Stripping debug symbols..."
strip "${APP_BUNDLE}/Contents/Resources/${BINARY_NAME}"
echo "📏 Stripped binary size: $(du -h ${APP_BUNDLE}/Contents/Resources/${BINARY_NAME} | cut -f1)"

# Create a launcher script that opens Terminal
echo "🚀 Creating Terminal launcher..."
cat > "${APP_BUNDLE}/Contents/MacOS/${APP_NAME}" << 'LAUNCHER_EOF'
#!/bin/bash
# Launcher script that opens Terminal with the actual binary

# Get the directory where this script is located (inside .app/Contents/MacOS/)
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
RESOURCES_DIR="${DIR}/../Resources"
BINARY_PATH="${RESOURCES_DIR}/16_full_metal_app"

# Get the absolute path to the binary
BINARY_ABSOLUTE="$(cd "${RESOURCES_DIR}" && pwd)/16_full_metal_app"

# Clean up old launcher scripts (older than 1 hour)
find /tmp -name "talka_launcher.*.command" -mmin +60 -delete 2>/dev/null || true

# Create a temporary script that Terminal will execute
# Use mktemp with a suffix to ensure .command extension
TEMP_SCRIPT=$(mktemp /tmp/talka_launcher.XXXXXX)
mv "${TEMP_SCRIPT}" "${TEMP_SCRIPT}.command"
TEMP_SCRIPT="${TEMP_SCRIPT}.command"
chmod +x "${TEMP_SCRIPT}"

# Write the binary path directly into the temp script
cat > "${TEMP_SCRIPT}" << EOF
#!/bin/bash
# Auto-generated launcher script for TalkaCapturePro

BINARY="${BINARY_ABSOLUTE}"
APP_BUNDLE="\${BINARY%/Contents/Resources/*}"

# Check if binary exists and is executable
if [[ ! -f "\${BINARY}" ]]; then
    echo "❌ Error: Binary not found: \${BINARY}"
    echo "Please ensure TalkaCapturePro.app is installed correctly"
    read -p "Press Enter to exit..."
    exit 1
fi

if [[ ! -x "\${BINARY}" ]]; then
    echo "❌ Error: Binary is not executable: \${BINARY}"
    echo "Try running: chmod +x \"\${BINARY}\""
    read -p "Press Enter to exit..."
    exit 1
fi

echo "🚀 Launching TalkaCapturePro..."
echo "📍 Binary: \${BINARY}"
echo ""

# Check if the app is quarantined by Gatekeeper
echo "🔍 Checking macOS Gatekeeper status..."
if xattr "\${APP_BUNDLE}" 2>/dev/null | grep -q "com.apple.quarantine"; then
    echo "⚠️  App is quarantined by macOS Gatekeeper"
    echo ""
    echo "This is normal for apps transferred from another computer."
    echo "Removing quarantine attribute..."
    echo ""
    
    # Remove quarantine from entire app bundle
    xattr -cr "\${APP_BUNDLE}" 2>/dev/null
    
    if [ \$? -eq 0 ]; then
        echo "✅ Quarantine removed successfully!"
        echo ""
    else
        echo "❌ Failed to remove quarantine automatically"
        echo ""
        echo "Please run this command in Terminal:"
        echo "  xattr -cr \"\${APP_BUNDLE}\""
        echo ""
        read -p "Press Enter to exit..."
        exit 1
    fi
else
    echo "✅ No quarantine detected"
    echo ""
fi

# Verify the binary signature (if signed)
echo "🔐 Verifying code signature..."
if codesign --verify --deep --strict "\${BINARY}" 2>/dev/null; then
    echo "✅ Code signature valid"
    echo ""
elif codesign --verify "\${APP_BUNDLE}" 2>/dev/null; then
    echo "✅ App bundle signature valid"
    echo ""
else
    echo "⚠️  Not code signed (this is OK for local testing)"
    echo ""
fi

# Run the binary
echo "▶️  Starting application..."
echo ""
"\${BINARY}"

# Keep terminal open if there was an error
if [ \$? -ne 0 ]; then
    echo ""
    echo "❌ Application exited with an error"
    read -p "Press Enter to close..."
fi

# Clean up this temporary script
rm -f "\$0"
EOF

# Open the temporary script in Terminal
open -a Terminal.app "${TEMP_SCRIPT}"
LAUNCHER_EOF

# Make the launcher executable
chmod +x "${APP_BUNDLE}/Contents/MacOS/${APP_NAME}"

# Create Info.plist FIRST (required for proper .app bundle)
echo "📝 Creating Info.plist..."
cat > "${APP_BUNDLE}/Contents/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDisplayName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleExecutable</key>
    <string>${APP_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>ai.talka.capturepro</string>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSMinimumSystemVersion</key>
    <string>15.0</string>
    <key>NSCameraUsageDescription</key>
    <string>This app needs screen recording permission to capture your screen.</string>
    <key>NSMicrophoneUsageDescription</key>
    <string>This app needs microphone access to capture audio.</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSBackgroundOnly</key>
    <false/>
    <key>LSUIElement</key>
    <false/>
</dict>
</plist>
EOF

# Create README for users
cat > "${DIST_DIR}/README.txt" << EOF
${APP_NAME} v${VERSION}
================================

Installation:
1. Copy ${APP_NAME}.app to /Applications folder (or run from anywhere)
2. Double-click to run (will open in Terminal)

First Run:
- The app will open in a Terminal window
- On first launch, it will automatically remove macOS Gatekeeper quarantine
- You will be prompted to authenticate with Auth0
- Follow the on-screen instructions in the Terminal to log in
- Your authentication will be saved for future runs

Note: This is a terminal-based application that requires Terminal access

macOS Gatekeeper:
- When you download or transfer this app, macOS marks it as "quarantined"
- The app will automatically detect and remove this quarantine on first launch
- This is normal behavior and ensures the app can run properly

Token Storage:
- Authentication tokens are stored in: ~/.talka_tokens.json
- Tokens are preserved across app restarts
- Each user has their own tokens

macOS Permissions:
- Screen Recording permission required (System Settings → Privacy & Security → Screen Recording)
- Microphone permission if using audio capture

Troubleshooting:
- If authentication fails, delete ~/.talka_tokens.json and restart
- Check System Settings for Screen Recording permissions
- Ensure you have an internet connection for authentication
- If Terminal opens but nothing happens:
  * The app is likely quarantined by macOS
  * It should remove quarantine automatically
  * If that fails, manually run: xattr -cr "/Applications/${APP_NAME}.app"
  * Then double-click the app again

Support: support@talka.ai
================================
EOF

# Optional: Code sign the binary
echo ""
CODE_SIGNED=false
if [[ -n "${APPLE_ID}" && -n "${APPLE_PASSWORD}" && -n "${TEAM_ID}" ]]; then
    echo "🔐 Found Apple credentials in environment, proceeding with code signing..."
    
    # Find the Developer ID Application certificate
    IDENTITY=$(security find-identity -v -p codesigning | grep "Developer ID Application" | head -1 | grep -o '"[^"]*"' | sed 's/"//g')
    
    if [[ -z "${IDENTITY}" ]]; then
        echo "⚠️  No Developer ID Application certificate found. Skipping code signing."
        echo "    Install a certificate from Apple Developer or skip signing for local distribution."
        echo ""
        echo "⚠️  WARNING: Without code signing, the app will show 'damaged' error on other Macs!"
        echo "    To distribute to other users, you MUST:"
        echo "      1. Get a Developer ID certificate from developer.apple.com"
        echo "      2. Install it in your Keychain"
        echo "      3. Re-run this script"
    else
        echo "🔐 Signing with identity: ${IDENTITY}"
        
        # Sign the actual binary first
        echo "  📝 Signing binary..."
        codesign --force --sign "${IDENTITY}" --options runtime --timestamp "${APP_BUNDLE}/Contents/Resources/${BINARY_NAME}"
        
        # Then sign the launcher script
        echo "  📝 Signing launcher..."
        codesign --force --sign "${IDENTITY}" --options runtime --timestamp "${APP_BUNDLE}/Contents/MacOS/${APP_NAME}"
        
        # Finally sign the entire .app bundle
        echo "  📝 Signing app bundle..."
        codesign --force --sign "${IDENTITY}" --options runtime --timestamp "${APP_BUNDLE}"
        echo "✅ App bundle signed successfully"
        
        # Verify signature with strict checking
        codesign --verify --deep --strict --verbose=2 "${APP_BUNDLE}"
        if [ $? -eq 0 ]; then
            echo "✅ Signature verified"
            CODE_SIGNED=true
        else
            echo "❌ Signature verification failed!"
            exit 1
        fi
    fi
else
    read -p "🔐 Code sign the app? (requires Apple Developer ID) (y/n) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        read -p "Enter your Developer ID Application identity: " IDENTITY
        
        # Sign the actual binary first
        echo "  📝 Signing binary..."
        codesign --force --sign "${IDENTITY}" --options runtime --timestamp "${APP_BUNDLE}/Contents/Resources/${BINARY_NAME}"
        
        # Then sign the launcher script
        echo "  📝 Signing launcher..."
        codesign --force --sign "${IDENTITY}" --options runtime --timestamp "${APP_BUNDLE}/Contents/MacOS/${APP_NAME}"
        
        # Finally sign the entire .app bundle
        echo "  📝 Signing app bundle..."
        codesign --force --sign "${IDENTITY}" --options runtime --timestamp "${APP_BUNDLE}"
        echo "✅ App bundle signed with: ${IDENTITY}"
        
        # Verify signature with strict checking
        codesign --verify --deep --strict --verbose=2 "${APP_BUNDLE}"
        if [ $? -eq 0 ]; then
            echo "✅ Signature verified"
            CODE_SIGNED=true
        else
            echo "❌ Signature verification failed!"
            exit 1
        fi
    fi
fi

# Optional: Notarize the binary
if [[ -n "${APPLE_ID}" && -n "${APPLE_PASSWORD}" && -n "${TEAM_ID}" ]]; then
    echo ""
    
    # Check if binary is code signed before attempting notarization
    if [[ "${CODE_SIGNED}" != "true" ]]; then
        echo "⚠️  Skipping notarization - binary is not code signed"
        echo "    Notarization requires a properly code-signed binary"
        echo ""
        echo "❌ WARNING: This binary will NOT work on other Macs!"
        echo "   Users will see: 'application is damaged and can't be opened'"
    else
        echo "📝 Notarizing app bundle with Apple..."
        
        # Create a temporary zip for notarization (entire .app bundle)
        NOTARIZE_ZIP="${DIST_DIR}/${APP_NAME}-notarize.zip"
        cd "${DIST_DIR}"
        ditto -c -k --keepParent "${APP_NAME}.app" "${APP_NAME}-notarize.zip"
        cd - > /dev/null
        
        # Submit for notarization
        echo "⏳ Submitting to Apple for notarization (this may take a few minutes)..."
        NOTARIZE_OUTPUT=$(xcrun notarytool submit "${NOTARIZE_ZIP}" \
            --apple-id "${APPLE_ID}" \
            --password "${APPLE_PASSWORD}" \
            --team-id "${TEAM_ID}" \
            --wait 2>&1)
        
        echo "${NOTARIZE_OUTPUT}"
        
        # Check if notarization was accepted
        if echo "${NOTARIZE_OUTPUT}" | grep -q "status: Accepted"; then
            echo "✅ Notarization successful!"
            
            # Staple the notarization ticket to the .app bundle
            echo "📎 Stapling notarization ticket to app bundle..."
            if xcrun stapler staple "${APP_BUNDLE}"; then
                echo "✅ Notarization ticket stapled successfully!"
                echo "   App will now work offline on any Mac without internet verification"
            else
                echo "⚠️  Warning: Could not staple ticket"
                echo "   App will still work but requires internet connection for first launch"
            fi
        else
            echo ""
            echo "❌ Notarization FAILED!"
            echo "⚠️  The app is NOT notarized and will show 'damaged' error on other Macs."
            echo ""
            echo "Common reasons for notarization failure:"
            echo "  1. App bundle is not properly code signed"
            echo "  2. Code signature doesn't have 'runtime' hardened option"
            echo "  3. App uses restricted entitlements"
            echo "  4. Info.plist is missing or malformed"
            echo ""
            echo "To get detailed failure reason, extract the submission ID from above and run:"
            echo "  xcrun notarytool log <submission-id> --apple-id ${APPLE_ID} --password ${APPLE_PASSWORD} --team-id ${TEAM_ID}"
            exit 1
        fi
        
        # Clean up temporary zip
        rm -f "${NOTARIZE_ZIP}"
    fi
fi

# Create ZIP for distribution
echo ""
echo "📦 Creating distribution archive..."
cd dist
zip -r "${APP_NAME}-${VERSION}.zip" "${APP_NAME}-${VERSION}"
cd ..

echo ""
echo "✅ Distribution package ready!"
echo "📦 Package: dist/${APP_NAME}-${VERSION}.zip"
echo "📁 Contents: dist/${APP_NAME}-${VERSION}/"
echo ""
echo "Distribution includes:"
echo "  - ${APP_NAME}.app (application bundle)"
echo "  - README.txt"
echo ""
if [[ "${CODE_SIGNED}" == "true" ]]; then
    echo "🚀 You can now distribute dist/${APP_NAME}-${VERSION}.zip to users"
    echo ""
    echo "✅ App is properly signed and notarized for distribution"
    echo ""
    echo "Users can:"
    echo "  1. Download and unzip the package"
    echo "  2. Drag ${APP_NAME}.app to their Applications folder"
    echo "  3. Double-click to launch"
    echo ""
    echo "⚠️  Note: On first launch, users may need to:"
    echo "    - Right-click the app and select 'Open' (first time only)"
    echo "    - Or go to System Settings → Privacy & Security → 'Open Anyway'"
else
    echo "⚠️  LOCAL USE ONLY - NOT FOR DISTRIBUTION"
    echo ""
    echo "❌ This app is NOT code signed and will show 'damaged' error on other Macs"
    echo ""
    echo "To distribute to other users, you need:"
    echo "  1. Apple Developer Account (https://developer.apple.com)"
    echo "  2. Developer ID Application certificate installed in Keychain"
    echo "  3. Set environment variables: APPLE_ID, APPLE_PASSWORD (app-specific), TEAM_ID"
    echo "  4. Re-run this script to code sign and notarize"
    echo ""
    echo "For local testing only, users can bypass Gatekeeper with:"
    echo "  xattr -cr ${APP_NAME}.app  (run in terminal before opening)"
fi

