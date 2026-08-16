# Architecture

## Dependency rule

```text
UI
 ↓
Core Editor
 ↓
Files / Formats / Search / Storage
 ↓
Optional Semantic / Research / Notebook Services
 ↓
Security Policy Constraints
```

The UI may call services through typed interfaces. The semantic layer may not become a prerequisite for basic text editing.

## Proposed workspace

- `bp-core`
- `bp-editor`
- `bp-buffer`
- `bp-files`
- `bp-formats`
- `bp-search`
- `bp-semantic`
- `bp-naming`
- `bp-organize`
- `bp-storage`
- `bp-history`
- `bp-clipboard`
- `bp-notebook`
- `bp-execution`
- `bp-data`
- `bp-research`
- `bp-security`
- `bp-crypto`
- `bp-secrets`
- `bp-integrity`
- `bp-redaction`
- `bp-audit`
- `bp-theme`
- `bp-platform`
- `bp-platform-windows`
- `bp-platform-linux`
- `bp-ui`
