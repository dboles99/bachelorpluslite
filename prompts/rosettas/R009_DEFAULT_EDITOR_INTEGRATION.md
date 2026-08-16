# Rosetta R009: Default Editor Integration

Implement supported default-editor registration.

Windows 10/11:
- register application capabilities and supported file types
- appear in Open With / Default Apps
- provide UI handoff to Windows supported default-app settings
- never replace or patch `notepad.exe`

Linux:
- install `.desktop` entry
- register supported MIME types
- use standards-based MIME/default-app mechanisms
- verify resulting association

Add association presets:
Notepad Replacement, Text + Notes, Text + Structured Data, Developer, Everything Supported, Custom.
