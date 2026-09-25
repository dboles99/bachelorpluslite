### Downloads

| Platform | File |
| --- | --- |
| Windows 10 / 11 (x86-64) | `bachelorpad-lite-VERSION-windows-x86_64.zip` |
| Linux (x86-64) | `bachelorpad-lite-VERSION-linux-x86_64.tar.gz` |

There is no installer, and that is a decision ([ADR-0067](../docs/decisions/ADR-0067.md)):
unpack the archive wherever you want it and run `bachelorpad`, or `bpad`.

macOS is not built. [ADR-0072](../docs/decisions/ADR-0072.md) says why, and
what it would cost.

### These archives are unsigned

Windows SmartScreen will warn on download. That is deferred code signing
([ADR-0055](../docs/decisions/ADR-0055.md)), answered rather than overlooked
-- and self-signing was refused outright, because a self-signed certificate is
only satisfied once you install a root certificate you have no reason to
trust.

`SHA256SUMS.txt` is attached. It answers tampering in transit, which is the
threat a certificate answers worst:

```sh
sha256sum -c SHA256SUMS.txt
```

### Licence

GPL-3.0-only ([ADR-0071](../docs/decisions/ADR-0071.md)) -- and `-only` rather
than `-or-later` is the decision, because Slint's grant is to version 3 and no
other.

`LICENSE` and `THIRD-PARTY-NOTICES.md` are inside each archive; the notices
list every crate linked into the binary.

### Documentation

`app-help/` ships inside the archive and **Help ▸ User Guide** opens it as a
document -- there is no browser involved, because this product launches no
programs. The same pages are at <https://bpad.prompt-forge.dev/docs> and in
[the wiki](https://github.com/dboles99/bachelorpluslite/wiki).
