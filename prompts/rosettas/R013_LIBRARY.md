# Rosetta R013: Put the Decision in a Library Crate

Second step. The rule this repository is built on: **decisions live in `bp-*`
crates, the shell only connects them.** A crate tested without a window is a
crate whose behaviour is actually known.

## Shape

- One crate. If it needs two, one of them is the wrong home.
- Pure where it can be: take the clock, the policy and the metrics as
  parameters rather than reading them. `bp-naming` earns its exhaustive tests
  by never reading a clock; `bp-editor::view` by never reading a window.
- Public API is the smallest thing the shell needs. A field the shell sets
  directly is a decision that escaped.

## Tests

Write the property, not three more examples. The defects found by review in
this repository were all found by properties:

- *moving a line down and back returns the exact original bytes* — caught two
  defects that example tests written beside the feature missed;
- *a step out undoes a step in, at every size in range* — zoom;
- *the rows partition the line exactly* — word wrap;
- *each profile is at least as strict as the one before, on every axis* —
  security profiles.

Then check the property can fail. Break the code by one step and watch the
test go red. **A property test that cannot fail is worse than no test**,
because it is counted as coverage.

Cover, always:

- the boundary values, and one past them;
- the empty case;
- the case where a caller passes something the type permits and the domain
  does not (a zero width, an index past the end);
- what the user is told when it fails.

## Done when

- the crate compiles alone and its tests pass alone;
- no `bp-ui` change was needed to test any of it;
- every public item has a doc comment saying *why*, not what.
