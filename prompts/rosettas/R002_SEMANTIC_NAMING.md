# Rosetta R002: Semantic Naming Engine

Implement or improve BachelorPad+ semantic filename generation.

Required grammar:
`<Semantic Title>_<DDMMMYYYY>.<extension>`

Default date source: creation date.

Requirements:
- deterministic naming works without an LLM
- optional local/cloud model providers are adapters
- sanitize invalid filenames
- expose confidence and naming signals
- preserve extension
- collision suffix `_02`
- intentional revision suffix `_v2`
- never physically rename without user approval
- unit/property tests for dates, invalid characters, collisions and Unicode
