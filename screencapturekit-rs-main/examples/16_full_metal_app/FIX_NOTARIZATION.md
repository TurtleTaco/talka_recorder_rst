# Fixed: Notarization Invalid Status

## What Was Wrong

Your build was failing notarization with "status: Invalid" because:

1. **Using `--deep` flag** - This is for app bundles, not standalone executables
2. **No timestamping** - Apple requires `--timestamp` for notarization
3. **Info.plist placement** - Having Info.plist in the directory during signing confused the process
4. **Weak verification** - The script wasn't catching signature problems early

## What I Fixed

### 1. Improved Code Signing
- ✅ Removed `--deep` flag (only for bundles)
- ✅ Added `--timestamp` flag (required for notarization)
- ✅ Added strict verification that fails fast if signature is invalid

### 2. Reorganized Build Process
**Old order:**
1. Strip binary
2. Create Info.plist ❌ (caused confusion)
3. Sign binary
4. Notarize

**New order:**
1. Strip binary
2. Sign binary (no Info.plist present)
3. Notarize
4. Create Info.plist ✅ (after notarization)
5. Create distribution ZIP

### 3. Better Error Detection
- Now verifies signature with `--strict` flag
- Exits immediately if signature is invalid
- Clearer error messages

## Try Again

### Step 1: Clean the old build

```bash
cd /Users/x/Documents/git/talka_recorder_rst/screencapturekit-rs-main
rm -rf dist/
```

### Step 2: Set environment variables

```bash
export APPLE_ID="zac.amazonprime@gmail.com"
export APPLE_PASSWORD="dars-yjvj-qnwm-pxpn"
export TEAM_ID="NG3QJY34ZY"
```

### Step 3: Run the fixed build script

```bash
cd examples/16_full_metal_app
./build_and_distribute.sh
```

## What to Look For

The build should now show:

```
✅ Binary signed successfully
✅ Signature verified
📝 Notarizing binary with Apple...
⏳ Submitting to Apple for notarization...
Current status: In Progress...
  status: Accepted  ✅ (not "Invalid"!)
✅ Notarization successful!
```

If you see `status: Accepted`, the notarization worked and your app will run on other Macs!

## If It Still Fails

### Option 1: Install Apple Intermediate Certificates

The "Authority=(unavailable)" in your code signature suggests missing intermediate certificates:

```bash
./install_apple_certificates.sh
```

This will download and install:
- Developer ID G1 & G2 certificates
- Apple WWDR CA certificates (G1, G2, G3)

### Option 2: Get Detailed Error Log

If notarization still fails, get the detailed log:

```bash
# Replace SUBMISSION_ID with the ID from build output
xcrun notarytool log SUBMISSION_ID \
  --apple-id "zac.amazonprime@gmail.com" \
  --password "dars-yjvj-qnwm-pxpn" \
  --team-id "NG3QJY34ZY"
```

This will show exactly what Apple didn't like about your binary.

## Build Speed Optimization

I also made clean builds optional (they were slowing you down):

```bash
# Normal build (fast, incremental)
./build_and_distribute.sh

# Clean build (slower, from scratch)
CLEAN_BUILD=true ./build_and_distribute.sh
```

## Testing on Another Mac

Once you see "status: Accepted":
1. Send `dist/TalkaCapturePro-1.0.0.zip` to another Mac
2. Unzip it
3. Double-click `TalkaCapturePro`
4. It should open without "damaged" error! 🎉

The first time, macOS will verify with Apple (requires internet), then it will remember the app is safe.

## References

- Apple Code Signing: https://developer.apple.com/documentation/security/code_signing_services
- Notarization Guide: https://developer.apple.com/documentation/security/notarizing_macos_software_before_distribution
- Troubleshooting: https://developer.apple.com/documentation/security/notarizing_macos_software_before_distribution/resolving_common_notarization_issues

