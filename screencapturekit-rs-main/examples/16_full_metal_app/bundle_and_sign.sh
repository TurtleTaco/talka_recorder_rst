#!/bin/bash
# Build script using cargo-bundle for distributing the 16_full_metal_app on macOS
#
# Usage:
#   ./bundle_and_sign.sh              # Normal build
#   ./bundle_and_sign.sh --clean      # Clean build from scratch

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_NAME="TalkaRecall"
VERSION="1.0.0"
BINARY_NAME="16_full_metal_app"

echo "🔨 Building ${APP_NAME} for macOS using cargo-bundle..."

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
    rm -rf target/release/bundle 2>/dev/null || true
    rm -rf dist 2>/dev/null || true
else
    echo "⏭️  Skipping clean (use --clean flag to clean first)"
fi

# Navigate to project root
cd /Users/linsun/Desktop/talka_recorder_rst/screencapturekit-rs-main

# Build using cargo-bundle
echo "🏗️  Building with cargo-bundle (release mode, macos_15_0 features)..."
cargo bundle --example ${BINARY_NAME} --release --features macos_15_0 --format osx

# The .app bundle will be at (cargo-bundle uses the package name):
CARGO_BUNDLE_PATH="target/release/examples/bundle/osx/screencapturekit.app"

if [[ ! -d "${CARGO_BUNDLE_PATH}" ]]; then
    echo "❌ Error: Bundle not created at expected path: ${CARGO_BUNDLE_PATH}"
    echo "Checking available bundles..."
    find target/release -name "*.app" -type d 2>/dev/null || echo "No .app bundles found"
    exit 1
fi

# Rename the app to our desired name
BUNDLE_PATH="target/release/examples/bundle/osx/${APP_NAME}.app"
if [[ "${CARGO_BUNDLE_PATH}" != "${BUNDLE_PATH}" ]]; then
    echo "📝 Renaming app bundle to ${APP_NAME}.app..."
    rm -rf "${BUNDLE_PATH}" 2>/dev/null || true
    mv "${CARGO_BUNDLE_PATH}" "${BUNDLE_PATH}"
fi

echo "✅ Bundle created successfully!"
echo "📦 Bundle location: ${BUNDLE_PATH}"

# Get bundle size
BUNDLE_SIZE=$(du -sh "${BUNDLE_PATH}" | cut -f1)
echo "📏 Bundle size: ${BUNDLE_SIZE}"

# Display bundle structure
echo ""
echo "📂 Bundle structure:"
echo "   ${APP_NAME}.app/"
echo "   ├── Contents/"
echo "   │   ├── MacOS/${BINARY_NAME}"
echo "   │   └── Info.plist"

# Update Info.plist with additional permissions and fixes
echo ""
echo "📝 Updating Info.plist..."

# Update bundle metadata
/usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName ${APP_NAME}" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null
/usr/libexec/PlistBuddy -c "Set :CFBundleName ${APP_NAME}" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier ai.talka.recall" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString ${VERSION}" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null

# Remove incompatible legacy flags that cause "not compatible" errors
echo "🔧 Removing legacy compatibility flags..."
/usr/libexec/PlistBuddy -c "Delete :LSRequiresCarbon" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Delete :CSResourcesFileMapped" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null || true

# Add privacy permissions
/usr/libexec/PlistBuddy -c "Add :NSCameraUsageDescription string 'This app needs screen recording permission to capture your screen.'" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null || \
/usr/libexec/PlistBuddy -c "Set :NSCameraUsageDescription 'This app needs screen recording permission to capture your screen.'" "${BUNDLE_PATH}/Contents/Info.plist"

/usr/libexec/PlistBuddy -c "Add :NSMicrophoneUsageDescription string 'This app needs microphone access to capture audio.'" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null || \
/usr/libexec/PlistBuddy -c "Set :NSMicrophoneUsageDescription 'This app needs microphone access to capture audio.'" "${BUNDLE_PATH}/Contents/Info.plist"

/usr/libexec/PlistBuddy -c "Add :NSHighResolutionCapable bool true" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null || \
/usr/libexec/PlistBuddy -c "Set :NSHighResolutionCapable true" "${BUNDLE_PATH}/Contents/Info.plist"

# Set minimum macOS version (14.0 for broad compatibility)
/usr/libexec/PlistBuddy -c "Add :LSMinimumSystemVersion string '14.0'" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null || \
/usr/libexec/PlistBuddy -c "Set :LSMinimumSystemVersion '14.0'" "${BUNDLE_PATH}/Contents/Info.plist"

# Add application category
/usr/libexec/PlistBuddy -c "Add :LSApplicationCategoryType string 'public.app-category.developer-tools'" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null || \
/usr/libexec/PlistBuddy -c "Set :LSApplicationCategoryType 'public.app-category.developer-tools'" "${BUNDLE_PATH}/Contents/Info.plist"

echo "✅ Info.plist updated"

# Copy app icon if it exists
ICON_FILE="${SCRIPT_DIR}/assets/talka_logo.icns"
if [[ -f "${ICON_FILE}" ]]; then
    echo ""
    echo "🎨 Adding app icon..."
    mkdir -p "${BUNDLE_PATH}/Contents/Resources"
    cp "${ICON_FILE}" "${BUNDLE_PATH}/Contents/Resources/${APP_NAME}.icns"
    # Update Info.plist to reference the icon
    /usr/libexec/PlistBuddy -c "Add :CFBundleIconFile string '${APP_NAME}.icns'" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null || \
    /usr/libexec/PlistBuddy -c "Set :CFBundleIconFile '${APP_NAME}.icns'" "${BUNDLE_PATH}/Contents/Info.plist"
    echo "✅ App icon added"
else
    echo ""
    echo "⚠️  Warning: Icon file not found at ${ICON_FILE}"
    echo "   Run ./create_icon.sh to generate the icon from SVG"
fi

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
        
        # Path to entitlements file
        ENTITLEMENTS_FILE="${SCRIPT_DIR}/TalkaRecall.entitlements"
        
        if [[ ! -f "${ENTITLEMENTS_FILE}" ]]; then
            echo "❌ Error: Entitlements file not found at ${ENTITLEMENTS_FILE}"
            echo "   Entitlements are required for screen recording and audio permissions"
            exit 1
        fi
        
        # Sign the binary first with the correct identifier
        echo "  📝 Signing binary with identifier: ai.talka.recall..."
        codesign --force --sign "${IDENTITY}" \
            --identifier "ai.talka.recall" \
            --options runtime \
            --timestamp \
            --entitlements "${ENTITLEMENTS_FILE}" \
            "${BUNDLE_PATH}/Contents/MacOS/${BINARY_NAME}"
        
        # Then sign the entire .app bundle
        echo "  📝 Signing app bundle with entitlements..."
        codesign --force --sign "${IDENTITY}" \
            --identifier "ai.talka.recall" \
            --options runtime \
            --timestamp \
            --entitlements "${ENTITLEMENTS_FILE}" \
            "${BUNDLE_PATH}"
        echo "✅ App bundle signed successfully"
        
        # Verify signature with strict checking
        codesign --verify --deep --strict --verbose=2 "${BUNDLE_PATH}"
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
        
        # Path to entitlements file
        ENTITLEMENTS_FILE="${SCRIPT_DIR}/TalkaRecall.entitlements"
        
        if [[ ! -f "${ENTITLEMENTS_FILE}" ]]; then
            echo "❌ Error: Entitlements file not found at ${ENTITLEMENTS_FILE}"
            echo "   Entitlements are required for screen recording and audio permissions"
            exit 1
        fi
        
        # Sign the binary first with the correct identifier
        echo "  📝 Signing binary with identifier: ai.talka.recall..."
        codesign --force --sign "${IDENTITY}" \
            --identifier "ai.talka.recall" \
            --options runtime \
            --timestamp \
            --entitlements "${ENTITLEMENTS_FILE}" \
            "${BUNDLE_PATH}/Contents/MacOS/${BINARY_NAME}"
        
        # Then sign the entire .app bundle
        echo "  📝 Signing app bundle with entitlements..."
        codesign --force --sign "${IDENTITY}" \
            --identifier "ai.talka.recall" \
            --options runtime \
            --timestamp \
            --entitlements "${ENTITLEMENTS_FILE}" \
            "${BUNDLE_PATH}"
        echo "✅ App bundle signed with: ${IDENTITY}"
        
        # Verify signature with strict checking
        codesign --verify --deep --strict --verbose=2 "${BUNDLE_PATH}"
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
NOTARIZED=false
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
        NOTARIZE_ZIP="/tmp/${APP_NAME}-notarize.zip"
        cd "$(dirname "${BUNDLE_PATH}")"
        ditto -c -k --keepParent "$(basename "${BUNDLE_PATH}")" "${NOTARIZE_ZIP}"
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
            NOTARIZED=true
            
            # Staple the notarization ticket to the .app bundle
            echo "📎 Stapling notarization ticket to app bundle..."
            if xcrun stapler staple "${BUNDLE_PATH}"; then
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

# Create distribution directory
echo ""
echo "📦 Creating distribution package..."
DIST_DIR="dist/${APP_NAME}-${VERSION}"
rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}"

# Copy the .app bundle to dist
cp -R "${BUNDLE_PATH}" "${DIST_DIR}/"

# Create README for users
cat > "${DIST_DIR}/README.txt" << EOF
${APP_NAME} v${VERSION}
================================

Installation:
1. Copy ${APP_NAME}.app to /Applications folder (or run from anywhere)
2. Double-click to run

First Run:
- On first launch, you will be prompted to authenticate with Auth0
- Follow the on-screen instructions to log in
- Your authentication will be saved for future runs

macOS Gatekeeper:
- When you download or transfer this app, macOS marks it as "quarantined"
- If the app doesn't open, run this in Terminal:
  xattr -cr "/Applications/${APP_NAME}.app"

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
- If macOS blocks the app: xattr -cr "/Applications/${APP_NAME}.app"

Support: support@talka.ai
================================
EOF

# Create ZIP for distribution
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
    if [[ "${NOTARIZED}" == "true" ]]; then
        echo "🚀 You can now distribute dist/${APP_NAME}-${VERSION}.zip to users"
        echo ""
        echo "✅ App is properly signed AND notarized for distribution"
        echo ""
        echo "Users can:"
        echo "  1. Download and unzip the package"
        echo "  2. Drag ${APP_NAME}.app to their Applications folder"
        echo "  3. Double-click to launch (no warnings!)"
    else
        echo "⚠️  App is SIGNED but NOT NOTARIZED"
        echo ""
        echo "❌ This app will show 'malware' warnings on other Macs!"
        echo ""
        echo "To properly distribute, you need to notarize. Set these environment variables:"
        echo "  export APPLE_ID='your-apple-id@example.com'"
        echo "  export APPLE_PASSWORD='app-specific-password'"
        echo "  export TEAM_ID='YOUR_TEAM_ID'"
        echo ""
        echo "Then re-run: ./bundle_and_sign.sh"
        echo ""
        echo "For LOCAL/TESTING ONLY, users can bypass with:"
        echo "  xattr -cr ${APP_NAME}.app"
        echo "  (or right-click → Open)"
    fi
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

echo ""
echo "🎉 Done! You can now run the app from: ${DIST_DIR}/${APP_NAME}.app"

