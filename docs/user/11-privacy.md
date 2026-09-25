# Privacy and security

## The short version

**Nothing you type leaves your machine.** Core editing never depends on a
cloud or an AI service ([ADR-0006](../decisions/ADR-0006.md)), and this
product makes no network connection at all -- no telemetry, no update check,
no crash reporting, no analytics.

**This product executes nothing** ([ADR-0057](../decisions/ADR-0057.md)).
There is no scripting, no macro language, no cell execution and no
interpreter. Opening a document cannot run anything, because there is nothing
to run.

You can verify both. The source is public and GPL-3.0-only.

## What is written down, and how to change it

The product records some things about the documents you open. **The Privacy
menu governs it, along two axes and no others:**

1. **Whether a recovery journal is written.** The journal is what makes
   unsaved work survive a crash. It contains your document's text, on disk,
   until you save.
2. **Whether anything is recorded about a document at all** -- its path, its
   title, its keywords, in the notes index.

Those two are what is left of a larger profile model, narrowed to the axes
that actually govern something ([ADR-0064](../decisions/ADR-0064.md)). A
smaller editor should not know more about you than the larger one did.

Profiles are per document, so a note you are relaxed about and one you are not
can be treated differently in the same session.

### The four profiles, and exactly what each one does

| Profile | Recovery journal | Recorded about the document |
| --- | --- | --- |
| **Standard** | on, **unencrypted** | path, title and tags |
| **Private** | on, **unencrypted** | the path only |
| **Confidential** | none | nothing |
| **Maximum** | none | nothing |

The Privacy menu shows both of those as **readouts** -- greyed lines you
cannot click, saying what the current profile has decided. They are greyed
because they are answers, not buttons.

**The journal readout says "on, unencrypted" and that wording is
load-bearing.** There was an encrypted journal; [ADR-0064](../decisions/ADR-0064.md)
deleted the variant rather than quietly pointing it at plaintext, and
[ADR-0065](../decisions/ADR-0065.md) kept Private's journal *because* the row
names its form honestly. A test asserts that if the row ever stops saying so,
Private goes back to writing no journal at all.

**Privacy Mode** is a session-wide override that can only tighten, never
loosen -- and it *acts*: journals already written are removed when you turn it
on.

There is no setting that gives you crash recovery without writing your text
somewhere, because that is not a thing that can exist.

### Two controls, both real

A profile decides two things -- the two above -- and nothing else, and every
line in Tools > Security Inspector is something the program actually does.

It used to list six. Four of them -- embeddings, leaving the machine,
temporary files and wiping memory -- were shown as the policy in force and
enforced by nothing, so a Confidential document was told *"Temporary files:
never written"* while every save wrote one. They were removed rather than
explained ([ADR-0082](../decisions/ADR-0082.md)). Nothing leaves the machine
under any profile, because this product has no network code at all; and a
save writes a temporary file beside the document and renames it into place,
under every profile, because that is what makes a save survive a crash.

**Confidential and Maximum now do exactly the same thing**, and did before:
only the four removed lines ever told them apart.

## What is deliberately absent

**No encryption.** `.bpadx` encrypted documents, the audit log, secret
scanning, metadata redaction and document signing were all in this product and
were all removed ([ADR-0064](../decisions/ADR-0064.md)). They are features of
the full BachelorPad+.

**Do not open a `.bpadx` file with this build.** It opens as ciphertext --
bytes, not text. An earlier release registered `.bpadx` as a file type this
program handles, including in the preset called *Notepad Replacement*; that
was wrong and is fixed ([ADR-0069](../decisions/ADR-0069.md)).

**No password manager, no keychain integration.** What would have gone in them
was a signing key, and signing is gone.

## Files an older build may have left behind

If you have run an earlier version, two files may still exist and are no
longer read by anything:

- `%APPDATA%\bachelorpad\security-history.log`
- `%APPDATA%\bachelorpad\recent.toml` (the list has since moved)

**They are not deleted for you**, on purpose
([ADR-0070](../decisions/ADR-0070.md)). The first is a frozen record of every
privacy-profile change the product made while the audit log existed, and it is
your record of your own machine -- this product does not get to decide it is
worthless. A product that will not silently claim a file association does not
get to silently delete out of your profile either.

Delete them by hand if you want them gone. Nothing will miss them.

## Reporting a security problem

See [SECURITY.md](../../SECURITY.md). Please do not open a public issue for a
vulnerability.
