# Debugging TalkaCapturePro Launch Issues

## Error: "Check with the developer to make sure TalkaCapturePro works with this version of macOS"

This error typically means the app is **crashing on startup**, not a compatibility issue.

## Debugging Steps

### 1. Run from Terminal to See Actual Error
On your test machine, open Terminal and run:

```bash
cd /path/to/TalkaCapturePro.app/Contents/MacOS
./TalkaCapturePro
```

This will show you the **actual error message** instead of the generic macOS error.

### 2. Check Code Signature
Verify the app is properly signed:

```bash
codesign -dvvv TalkaCapturePro.app
spctl -a -vvv -t execute TalkaCapturePro.app
```

### 3. Check for Missing Dynamic Libraries
See what libraries the binary needs:

```bash
otool -L TalkaCapturePro.app/Contents/MacOS/TalkaCapturePro
```

### 4. Check Console for Crash Logs
1. Open **Console.app** on the test Mac
2. In the search bar, type: `TalkaCapturePro`
3. Try to launch the app
4. Look for crash reports or error messages

### 5. Clear Quarantine Attributes
Even though notarized, try clearing quarantine:

```bash
xattr -cr TalkaCapturePro.app
```

### 6. Check Architecture
Verify the binary architecture matches your test machine:

```bash
file TalkaCapturePro.app/Contents/MacOS/TalkaCapturePro
lipo -info TalkaCapturePro.app/Contents/MacOS/TalkaCapturePro
```

## Common Issues

### Missing Dynamic Libraries
If `otool -L` shows libraries with `@rpath` that don't exist, you may need to statically link or include those libraries.

### Rust Dependencies
Check if the binary depends on system libraries that aren't available on the test machine.

### Auth0 Configuration
The app might be failing to initialize Auth0. Check if the Auth0 credentials are properly embedded in the binary.

## Next Steps

1. Run the app from Terminal (step 1 above)
2. Copy the exact error message
3. Share the error message to diagnose the root cause

The terminal output will reveal the actual issue!

