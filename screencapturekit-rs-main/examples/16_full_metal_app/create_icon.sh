#!/bin/bash
# Script to convert talka_logo.svg to .icns icon for macOS app

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ASSETS_DIR="${SCRIPT_DIR}/assets"
SVG_FILE="${ASSETS_DIR}/square-crop.jpg"
ICON_NAME="talka_logo"
ICONSET_DIR="${ASSETS_DIR}/${ICON_NAME}.iconset"

echo "🎨 Creating macOS .icns icon from ${SVG_FILE}..."

# Check if SVG exists
if [[ ! -f "${SVG_FILE}" ]]; then
    echo "❌ Error: SVG file not found at ${SVG_FILE}"
    exit 1
fi

# Check if necessary tools are available
if ! command -v sips &> /dev/null && ! command -v rsvg-convert &> /dev/null; then
    echo "❌ Error: Neither 'sips' nor 'rsvg-convert' found."
    echo "Installing librsvg via Homebrew..."
    if command -v brew &> /dev/null; then
        brew install librsvg
    else
        echo "Please install Homebrew first: https://brew.sh"
        exit 1
    fi
fi

# Create iconset directory
rm -rf "${ICONSET_DIR}"
mkdir -p "${ICONSET_DIR}"

# Function to convert SVG to PNG at specific size
convert_to_png() {
    local size=$1
    local output_name=$2
    
    if command -v rsvg-convert &> /dev/null; then
        # Use rsvg-convert (better quality)
        rsvg-convert -w ${size} -h ${size} "${SVG_FILE}" -o "${ICONSET_DIR}/${output_name}"
        echo "  ✓ Created ${output_name} (${size}x${size})"
    else
        # Fallback to sips (macOS built-in, but needs intermediate PNG)
        # First convert SVG to large PNG, then resize
        local temp_png="${ICONSET_DIR}/temp_large.png"
        qlmanage -t -s ${size} -o "${ICONSET_DIR}" "${SVG_FILE}" > /dev/null 2>&1
        mv "${ICONSET_DIR}/$(basename ${SVG_FILE}).png" "${ICONSET_DIR}/${output_name}" 2>/dev/null || true
        
        if [[ ! -f "${ICONSET_DIR}/${output_name}" ]]; then
            echo "  ⚠️  Warning: Could not create ${output_name}"
        else
            echo "  ✓ Created ${output_name} (${size}x${size})"
        fi
    fi
}

# Generate all required icon sizes for macOS
echo ""
echo "📐 Generating icon sizes..."
convert_to_png 16 "icon_16x16.png"
convert_to_png 32 "icon_16x16@2x.png"
convert_to_png 32 "icon_32x32.png"
convert_to_png 64 "icon_32x32@2x.png"
convert_to_png 128 "icon_128x128.png"
convert_to_png 256 "icon_128x128@2x.png"
convert_to_png 256 "icon_256x256.png"
convert_to_png 512 "icon_256x256@2x.png"
convert_to_png 512 "icon_512x512.png"
convert_to_png 1024 "icon_512x512@2x.png"

# Create .icns file
echo ""
echo "🔨 Creating .icns file..."
iconutil -c icns "${ICONSET_DIR}" -o "${ASSETS_DIR}/${ICON_NAME}.icns"

if [[ -f "${ASSETS_DIR}/${ICON_NAME}.icns" ]]; then
    echo "✅ Icon created successfully: ${ASSETS_DIR}/${ICON_NAME}.icns"
    
    # Get file size
    ICON_SIZE=$(du -h "${ASSETS_DIR}/${ICON_NAME}.icns" | cut -f1)
    echo "📏 Icon size: ${ICON_SIZE}"
    
    # Clean up iconset directory
    echo ""
    echo "🧹 Cleaning up temporary files..."
    rm -rf "${ICONSET_DIR}"
    echo "✅ Done! Icon is ready to use."
else
    echo "❌ Error: Failed to create .icns file"
    exit 1
fi

echo ""
echo "📦 To use this icon in your app bundle, it will be automatically"
echo "   included when you run ./bundle_and_sign.sh"
