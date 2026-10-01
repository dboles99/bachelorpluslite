### Downloads

| You have | Download | Then |
| --- | --- | --- |
| **Windows 10 or 11** | `bachelorpad-lite-VERSION-windows-x86_64-setup.exe` | Run it. No administrator prompt; it installs for you alone |
| Windows, no installer | `bachelorpad-lite-VERSION-windows-x86_64.zip` | Unpack anywhere and run `bachelorpad.exe` |
| **Ubuntu, Debian, Mint, Pop!_OS** | `bachelorpad-lite_VERSION_amd64.deb` | `sudo apt install ./bachelorpad-lite_VERSION_amd64.deb` |
| **Fedora, openSUSE** | `bachelorpad-lite-VERSION-1.x86_64.rpm` | `sudo dnf install ./bachelorpad-lite-VERSION-1.x86_64.rpm` |
| Any other Linux | `bachelorpad-lite-VERSION-x86_64.AppImage` | `chmod +x` it and run it |
| Linux, no package | `bachelorpad-lite-VERSION-linux-x86_64.tar.gz` | Unpack and run `./bachelorpad` |
| Arch | `PKGBUILD` | For `makepkg -si`, or the AUR |

All of them are x86-64. **Windows** needs nothing installed first: the C
runtime is inside the executable. **Linux** needs glibc 2.35 or later --
Ubuntu 22.04, Debian 12, Linux Mint 21, Fedora 36, or anything newer
([ADR-0087](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0087.md)).

The installer offers, unticked, to put the program in **Open with** and in
**Settings > Default apps**. It never makes itself the default for anything;
that stays your choice, in Windows' own settings
([ADR-0012](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0012.md),
[ADR-0093](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0093.md)).

macOS is not built.
[ADR-0072](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0072.md)
says why, and what it would cost.

### These downloads are unsigned

Windows SmartScreen will warn when you download the installer and when you
first run it: choose **More info**, then **Run anyway**. On Windows 11 with
**Smart App Control** turned on, it may refuse to run at all. That is code
signing not yet in place, decided rather than overlooked
([ADR-0094](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0094.md)),
and self-signing was refused outright, because a self-signed certificate is
only satisfied once you install a root certificate you have no reason to
trust.

`SHA256SUMS.txt` is attached. It answers tampering in transit, which is the
threat a certificate answers worst:

```sh
sha256sum -c SHA256SUMS.txt --ignore-missing
```

```powershell
(Get-FileHash .\bachelorpad-lite-VERSION-windows-x86_64-setup.exe).Hash
```

### Licence

GPL-3.0-only
([ADR-0071](https://github.com/dboles99/bachelorpluslite/blob/main/docs/decisions/ADR-0071.md))
-- and `-only` rather than `-or-later` is the decision, because Slint's grant
is to version 3 and no other.

`LICENSE` and `THIRD-PARTY-NOTICES.md` are inside every download; the notices
list every crate linked into the binary.

### Documentation

`app-help/` ships with the program and **Help > User Guide** opens it as a
document -- there is no browser involved, because this product launches no
programs. The same pages are at <https://bpad.prompt-forge.dev/docs> and in
[the wiki](https://github.com/dboles99/bachelorpluslite/wiki).
