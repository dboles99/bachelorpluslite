# Rosetta R001: Implement a BachelorPad+ Feature

You are working in the BachelorPad+ Rust workspace.

## Read first
- `specs.md`
- `PROJECT_MEMORY.md`
- `DECISIONS.md`
- relevant ADRs
- relevant architecture document
- relevant task file

## Governance
Use:
`project -> task -> step -> prompts/rosettas -> artifacts -> metadata -> database -> note`
where useful.

## Constraints
- preserve Windows 10, Windows 11 and Linux targets
- do not introduce cloud dependency into core editing
- do not bypass document security policy
- do not silently rename/move user files
- do not auto-run executable notebook content
- do not invent cryptography
- keep UI thread non-blocking
- prefer typed interfaces and testable crates

## Task
Implement: `<FEATURE>`

## Required output
1. repository inspection findings
2. implementation plan
3. affected crates/files
4. code
5. tests
6. benchmark/security notes where relevant
7. updated docs/metadata
8. explicit remaining risks
