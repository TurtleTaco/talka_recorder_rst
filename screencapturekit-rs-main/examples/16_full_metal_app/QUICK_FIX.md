# Quick Fix: Get Code Signing Working

## Your Current Status

✅ Environment variables set (APPLE_ID, APPLE_PASSWORD, TEAM_ID)  
❌ **No Developer ID Application certificate installed in Keychain**

This is why your builds show "LOCAL USE ONLY - NOT FOR DISTRIBUTION"

## How to Fix (3 Steps)

### Step 1: Create Certificate Signing Request (CSR)

1. Open **Keychain Access** app (`/Applications/Utilities/Keychain Access.app`)
2. Menu: **Keychain Access → Certificate Assistant → Request a Certificate from a Certificate Authority**
3. Fill in:
   - Email: Your email
   - Common Name: Your name
   - Request: **Saved to disk**
   - ✅ Check "Let me specify key pair information"
4. Save the file (CertificateSigningRequest.certSigningRequest)
5. Choose: Key Size **2048 bits**, Algorithm **RSA**
6. Click Continue → Done

### Step 2: Get Certificate from Apple

1. Go to: https://developer.apple.com/account/resources/certificates/add
2. Select: **"Developer ID Application"** (under Software section)
   - ⚠️ NOT "Apple Development"
   - ⚠️ NOT "iOS Distribution"
3. Click Continue
4. Upload your `CertificateSigningRequest.certSigningRequest` file
5. Click Continue
6. **Download** the certificate (.cer file)

### Step 3: Install Certificate

1. **Double-click** the downloaded `.cer` file
2. It will open Keychain Access and install automatically
3. Verify it's in the "login" or "My Certificates" keychain

### Step 4: Verify Installation

```bash
cd screencapturekit-rs-main/examples/16_full_metal_app
./setup_certificate.sh
```

Should show: `✅ Found 1 Developer ID Application certificate`

### Step 5: Build Again

```bash
./build_and_distribute.sh
```

Now it should:
- ✅ Code sign the binary
- ✅ Notarize with Apple
- ✅ Create distributable package

## Build Optimization

I made the clean build optional. Now it skips cleaning by default for faster iteration.

To force a clean build:
```bash
CLEAN_BUILD=true ./build_and_distribute.sh
```

## Verification

After building, you should see:
- `🔐 Signing with identity: Developer ID Application: Your Name (TEAM_ID)`
- `✅ Binary signed successfully`
- `✅ Signature verified`
- `status: Accepted` (during notarization)
- `✅ Notarization successful!`

Then the ZIP file will work on other Macs without the "damaged" error!

## Troubleshooting

**If certificate doesn't show up after installation:**
```bash
# List all certificates
security find-identity -v

# Look for "Developer ID Application"
security find-identity -v -p codesigning
```

**If it's there but the script doesn't find it:**
- Make sure it's in "login" keychain, not "System"
- Try restarting Terminal to refresh Keychain access
- Check that the private key is also in Keychain (should be under "Keys")

**Still having issues?**
Run: `./setup_certificate.sh` for detailed step-by-step guidance

