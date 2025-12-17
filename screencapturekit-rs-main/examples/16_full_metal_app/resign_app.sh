#!/bin/bash
# Quick script to re-sign and re-notarize the existing app with correct identifier

set -e

APP_NAME="TalkaCapturePro"
VERSION="1.0.0"
BINARY_NAME="16_full_metal_app"
BUNDLE_IDENTIFIER="ai.talka.capturepro"

cd /Users/x/Documents/git/talka_recorder_rst/screencapturekit-rs-main

# Find or use the existing bundle
CARGO_BUNDLE_PATH="target/release/examples/bundle/osx/screencapturekit.app"
BUNDLE_PATH="target/release/examples/bundle/osx/${APP_NAME}.app"

if [[ ! -d "${BUNDLE_PATH}" ]] && [[ -d "${CARGO_BUNDLE_PATH}" ]]; then
    echo "📝 Renaming app bundle to ${APP_NAME}.app..."
    mv "${CARGO_BUNDLE_PATH}" "${BUNDLE_PATH}"
fi

if [[ ! -d "${BUNDLE_PATH}" ]]; then
    echo "❌ Error: Bundle not found at ${BUNDLE_PATH}"
    echo "Please run: cargo bundle --example 16_full_metal_app --release --features macos_15_0 --format osx"
    exit 1
fi

echo "📦 Found bundle: ${BUNDLE_PATH}"

# Update Info.plist
echo "📝 Updating Info.plist..."
/usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName ${APP_NAME}" "${BUNDLE_PATH}/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleName ${APP_NAME}" "${BUNDLE_PATH}/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier ${BUNDLE_IDENTIFIER}" "${BUNDLE_PATH}/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString ${VERSION}" "${BUNDLE_PATH}/Contents/Info.plist"

# Remove incompatible legacy flags
echo "🔧 Removing legacy compatibility flags..."
/usr/libexec/PlistBuddy -c "Delete :LSRequiresCarbon" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Delete :CSResourcesFileMapped" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null || true

# Add privacy permissions
/usr/libexec/PlistBuddy -c "Add :NSCameraUsageDescription string 'This app needs screen recording permission to capture your screen.'" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null || \
/usr/libexec/PlistBuddy -c "Set :NSCameraUsageDescription 'This app needs screen recording permission to capture your screen.'" "${BUNDLE_PATH}/Contents/Info.plist"

/usr/libexec/PlistBuddy -c "Add :NSMicrophoneUsageDescription string 'This app needs microphone access to capture audio.'" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null || \
/usr/libexec/PlistBuddy -c "Set :NSMicrophoneUsageDescription 'This app needs microphone access to capture audio.'" "${BUNDLE_PATH}/Contents/Info.plist"

/usr/libexec/PlistBuddy -c "Add :LSMinimumSystemVersion string '14.0'" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null || \
/usr/libexec/PlistBuddy -c "Set :LSMinimumSystemVersion '14.0'" "${BUNDLE_PATH}/Contents/Info.plist"

/usr/libexec/PlistBuddy -c "Add :LSApplicationCategoryType string 'public.app-category.developer-tools'" "${BUNDLE_PATH}/Contents/Info.plist" 2>/dev/null || \
/usr/libexec/PlistBuddy -c "Set :LSApplicationCategoryType 'public.app-category.developer-tools'" "${BUNDLE_PATH}/Contents/Info.plist"

echo "✅ Info.plist updated"

# Remove old signatures
echo "🧹 Removing old signatures..."
codesign --remove-signature "${BUNDLE_PATH}/Contents/MacOS/${BINARY_NAME}" 2>/dev/null || true
codesign --remove-signature "${BUNDLE_PATH}" 2>/dev/null || true

# Code sign
if [[ -z "${APPLE_ID}" || -z "${APPLE_PASSWORD}" || -z "${TEAM_ID}" ]]; then
    echo "❌ Error: Please set APPLE_ID, APPLE_PASSWORD, and TEAM_ID environment variables"
    echo ""
    echo "Example:"
    echo "  export APPLE_ID='your@email.com'"
    echo "  export APPLE_PASSWORD='app-specific-password'"
    echo "  export TEAM_ID='YOUR_TEAM_ID'"
    exit 1
fi

# Find the Developer ID Application certificate
IDENTITY=$(security find-identity -v -p codesigning | grep "Developer ID Application" | head -1 | grep -o '"[^"]*"' | sed 's/"//g')

if [[ -z "${IDENTITY}" ]]; then
    echo "❌ No Developer ID Application certificate found"
    exit 1
fi

echo "🔐 Signing with identity: ${IDENTITY}"

# Sign the binary first with the correct identifier
echo "  📝 Signing binary with identifier: ${BUNDLE_IDENTIFIER}..."
codesign --force --sign "${IDENTITY}" \
    --identifier "${BUNDLE_IDENTIFIER}" \
    --options runtime \
    --timestamp \
    "${BUNDLE_PATH}/Contents/MacOS/${BINARY_NAME}"

# Then sign the entire .app bundle
echo "  📝 Signing app bundle..."
codesign --force --sign "${IDENTITY}" \
    --identifier "${BUNDLE_IDENTIFIER}" \
    --options runtime \
    --timestamp \
    "${BUNDLE_PATH}"

echo "✅ App bundle signed successfully"

# Verify signature
echo "🔍 Verifying signature..."
codesign --verify --deep --strict --verbose=2 "${BUNDLE_PATH}"
if [ $? -eq 0 ]; then
    echo "✅ Signature verified"
else
    echo "❌ Signature verification failed!"
    exit 1
fi

# Show signature details
echo ""
echo "📋 Signature details:"
codesign -dvvv "${BUNDLE_PATH}" 2>&1 | grep -E "Identifier=|Authority=|TeamIdentifier="

# Notarize
echo ""
echo "📝 Notarizing app bundle with Apple..."

NOTARIZE_ZIP="/tmp/${APP_NAME}-notarize.zip"
cd "$(dirname "${BUNDLE_PATH}")"
ditto -c -k --keepParent "$(basename "${BUNDLE_PATH}")" "${NOTARIZE_ZIP}"
cd - > /dev/null

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
    
    # Staple the notarization ticket
    echo "📎 Stapling notarization ticket..."
    if xcrun stapler staple "${BUNDLE_PATH}"; then
        echo "✅ Notarization ticket stapled successfully!"
    else
        echo "⚠️  Warning: Could not staple ticket"
    fi
else
    echo "❌ Notarization FAILED!"
    
    # Try to get submission ID for logs
    SUBMISSION_ID=$(echo "${NOTARIZE_OUTPUT}" | grep -o 'id: [a-f0-9-]*' | head -1 | cut -d' ' -f2)
    if [[ -n "${SUBMISSION_ID}" ]]; then
        echo ""
        echo "Getting detailed logs..."
        xcrun notarytool log "${SUBMISSION_ID}" --apple-id "${APPLE_ID}" --password "${APPLE_PASSWORD}" --team-id "${TEAM_ID}"
    fi
    exit 1
fi

rm -f "${NOTARIZE_ZIP}"

# Create distribution
echo ""
echo "📦 Creating distribution package..."
DIST_DIR="dist/${APP_NAME}-${VERSION}"
rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}"

cp -R "${BUNDLE_PATH}" "${DIST_DIR}/"

cat > "${DIST_DIR}/README.txt" << EOF
${APP_NAME} v${VERSION}
================================

Installation:
1. Copy ${APP_NAME}.app to /Applications folder
2. Double-click to run

macOS Permissions:
- Screen Recording permission required
- Microphone permission if using audio capture

Support: support@talka.ai
================================
EOF

cd dist
zip -r "${APP_NAME}-${VERSION}.zip" "${APP_NAME}-${VERSION}"
cd ..

echo ""
echo "✅ Distribution ready!"
echo "📦 Package: dist/${APP_NAME}-${VERSION}.zip"
echo ""
echo "🚀 This app is properly signed and notarized for distribution!"

