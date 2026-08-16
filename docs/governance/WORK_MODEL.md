# Work Model

Canonical hierarchy:

```text
project
  -> task
    -> step
      -> prompts/rosettas
        -> artifacts
          -> metadata
            -> database
              -> note
```

Not every task requires every layer. Use layers only where they add traceability.

## Project

Represents a major product area or implementation phase.

## Task

A bounded objective with explicit acceptance criteria.

## Step

A testable implementation or research action.

## Prompt / Rosetta

A reusable instruction packet for Claude, Codex, ChatGPT or another coding/research agent.

## Artifact

Output created by the work: source, test, report, schema, benchmark, screenshot, package, migration, etc.

## Metadata

Machine-readable provenance, status, hashes, dates, dependencies and acceptance results.

## Database

Persistent structured storage when appropriate.

## Note

Human context, caveats, observations, rationale or follow-up information that does not belong in the formal decision layer.
