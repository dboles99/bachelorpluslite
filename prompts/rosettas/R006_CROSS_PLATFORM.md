# Rosetta R006: Cross-Platform Feature

Feature: `<FEATURE>`

Implement portable behavior in shared crates.
Isolate OS behavior behind `bp-platform` traits.
Add Windows and Linux adapters separately.
Do not introduce Windows APIs into portable core crates.
Document capability differences honestly.
Add platform-specific tests where automation is feasible.
