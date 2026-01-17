# Notarization Guide for Talka Recall

## Why You're Getting the "Malware" Warning

Your app is **code signed** but **NOT notarized**. macOS requires both for apps distributed outside the App Store.

- ✅ **Code Signing** = Proves you built it (DONE)
- ❌ **Notarization** = Apple verifies it's safe (MISSING)

Without notarization, users on other Macs see:
> "Apple could not verify TalkaRecall is free of malware that may harm your Mac..."

## How to Properly Notarize

### Step 1: Get Your Apple Developer Credentials

You need three pieces of information:

#### 1. Apple ID
Your Apple Developer account email (e.g., `your.email@icloud.com`)

#### 2. Team ID
Find this at: https://developer.apple.com/account
- Click on "Membership" in the sidebar
- Copy your "Team ID" (10-character code like `ABC123XYZ4`)

#### 3. App-Specific Password
**Important:** Don't use your regular Apple password!

Generate an app-specific password:
1. Go to https://appleid.apple.com
2. Sign in with your Apple ID
3. Under "Security" → "App-Specific Passwords" → Click "+"
4. Name it "Talka Recall Notarization"
5. Copy the generated password (format: `xxxx-xxxx-xxxx-xxxx`)

### Step 2: Set Environment Variables

Add these to your shell profile (`~/.zshrc` or `~/.bash_profile`):

```bash
export APPLE_ID="your.email@icloud.com"
export APPLE_PASSWORD="xxxx-xxxx-xxxx-xxxx"  # App-specific password
export TEAM_ID="ABC123XYZ4"
```

Then reload your shell:
```bash
source ~/.zshrc
```

### Step 3: Rebuild with Notarization

Now run the build script again:

```bash
cd /Users/linsun/Desktop/talka_recorder_rst/screencapturekit-rs-main/examples/16_full_metal_app
./bundle_and_sign.sh
```

This time it will:
1. Code sign the app ✅
2. Submit to Apple for notarization (takes 2-10 minutes) ⏳
3. Staple the notarization ticket to the app ✅

You'll see output like:
```
📝 Notarizing app bundle with Apple...
⏳ Submitting to Apple for notarization (this may take a few minutes)...
  id: abc123-def456-ghi789
  status: Accepted
✅ Notarization successful!
📎 Stapling notarization ticket to app bundle...
✅ Notarization ticket stapled successfully!
```

### Step 4: Verify It Worked

Check the app's notarization status:

```bash
spctl -a -vv -t install dist/TalkaRecall-1.0.0/TalkaRecall.app
```

Should show:
```
source=Notarized Developer ID
origin=Developer ID Application: YOUR NAME (ABC123XYZ4)
```

### Step 5: Distribute

The file `dist/TalkaRecall-1.0.0.zip` is now ready for distribution!

Users can:
- Double-click to open without any warnings ✅
- No need to right-click or use `xattr` commands ✅

## Quick Fix for Current Build (Without Notarization)

If you want to use the current signed (but not notarized) build on other Macs:

```bash
# On the other Mac, run this command:
xattr -cr /path/to/TalkaRecall.app

# Then open normally
open /path/to/TalkaRecall.app
```

Or:
- Right-click the app → "Open"
- Click "Open" in the warning dialog

**Note:** This is only for testing. For production distribution, you MUST notarize.

## Troubleshooting

### "Invalid credentials" error
- Double-check your Apple ID and Team ID
- Make sure you're using an **app-specific password**, not your regular password
- Verify your Apple Developer account is active

### "Notarization failed" error
Get detailed logs:
```bash
# From the submission output, copy the submission ID, then:
xcrun notarytool log <submission-id> \
  --apple-id "${APPLE_ID}" \
  --password "${APPLE_PASSWORD}" \
  --team-id "${TEAM_ID}"
```

### Still seeing warnings after notarization
- Clear the app from quarantine: `xattr -cr TalkaRecall.app`
- Make sure you distributed the ZIP file from `dist/`, not from `target/`
- Verify notarization: `spctl -a -vv -t install TalkaRecall.app`

## Cost

Notarization is **FREE** but requires:
- Apple Developer Program membership ($99/year)
- Or you can use a free Apple ID (with limitations for personal/testing use)

## Summary

For distribution to other Macs:

| Method | Works? | User Experience |
|--------|--------|-----------------|
| **No signing** | ❌ | "damaged and can't be opened" |
| **Signed only** | ⚠️ | "malware" warning (your current situation) |
| **Signed + Notarized** | ✅ | Opens without warnings! |

Set the environment variables and rebuild to get the ✅ experience!
