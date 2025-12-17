# macOS Distribution Guide - Code Signing & Notarization

## Problem: "Application is damaged" Error

When you build on your Mac and send the app to another Mac, users see:
> "TalkaCapturePro is damaged and can't be opened. You should move it to the Trash."

**Root Cause:** macOS Gatekeeper blocks unsigned/unnotarized applications downloaded from the internet.

## Why It Works on Your Mac

Your local Mac built the app and doesn't apply quarantine restrictions to locally-built binaries. Other Macs see it as a downloaded app and block it for security.

## Solution: Code Sign + Notarize

### Prerequisites

1. **Apple Developer Account** ($99/year)
   - Sign up at https://developer.apple.com

2. **Developer ID Application Certificate**
   - Log into Apple Developer portal
   - Go to: Certificates, Identifiers & Profiles → Certificates
   - Create: "Developer ID Application" certificate
   - Download and install in Keychain Access

3. **App-Specific Password** for notarization
   - Go to: https://appleid.apple.com
   - Sign in → Security → App-Specific Passwords
   - Generate new password (save it securely)

4. **Team ID**
   - Find in Apple Developer portal under Membership

### Setup Environment Variables

```bash
export APPLE_ID="your@email.com"
export APPLE_PASSWORD="xxxx-xxxx-xxxx-xxxx"  # app-specific password
export TEAM_ID="XXXXXXXXXX"  # your 10-character team ID
```

Add these to your `~/.zshrc` or `~/.bashrc` for persistence.

### Run the Build Script

```bash
cd screencapturekit-rs-main/examples/16_full_metal_app
./build_and_distribute.sh
```

The script will now:
1. ✅ Build the binary
2. ✅ Code sign with your Developer ID
3. ✅ Submit for notarization
4. ✅ Wait for Apple approval (2-10 minutes)
5. ✅ Staple the notarization ticket
6. ✅ Create distribution ZIP

### Verify It Worked

Check the build output for:
- `✅ Binary signed successfully`
- `✅ Signature verified`
- `status: Accepted` (not "Invalid")
- `✅ Notarization successful!`

### Test on Another Mac

1. Send the ZIP to another Mac (or AirDrop it)
2. Unzip and double-click
3. macOS will verify the signature with Apple
4. App should open without "damaged" error

## Troubleshooting

### Certificate Not Found

If you see: "No Developer ID Application certificate found"

```bash
# Check if certificate is installed
security find-identity -v -p codesigning

# You should see something like:
# "Developer ID Application: Your Name (TEAM_ID)"
```

If not found, re-download and install the certificate from Apple Developer portal.

### Notarization Failed (Invalid Status)

Common causes:
1. **Binary not code signed** - Must sign before notarizing
2. **Missing runtime hardening** - The script uses `--options runtime`
3. **Invalid certificate** - Must be "Developer ID Application" not "Apple Development"

Get detailed error logs:
```bash
# Find submission ID from build output, then:
xcrun notarytool log <submission-id> \
  --apple-id "your@email.com" \
  --password "xxxx-xxxx-xxxx-xxxx" \
  --team-id "XXXXXXXXXX"
```

### Stapling Failed

If you see: "Could not staple ticket"
- This is usually OK - the notarization is still valid
- macOS will check online if stapling fails
- Only matters for offline Macs

## Alternative: Local Testing Only

If you just want to test locally on another Mac without Apple Developer account:

**On the destination Mac**, run this before opening:
```bash
xattr -cr TalkaCapturePro
```

This removes the quarantine attribute, but **this is NOT for distribution** - each user would need to run this command.

## Cost Summary

- Apple Developer Account: $99/year
- Benefits: Distribute to unlimited users, proper macOS integration, no security warnings

## Questions?

- Apple Code Signing Guide: https://developer.apple.com/support/code-signing/
- Notarization Guide: https://developer.apple.com/documentation/security/notarizing_macos_software_before_distribution

