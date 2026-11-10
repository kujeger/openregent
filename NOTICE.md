# Licensing and provenance

openregent's own code and documentation are licensed under the
[GNU Affero General Public License version 3](LICENSE).

That grant does not cover Majesty itself: its executables, data files,
art, sound, text or scripts, nor anything extracted from or derived from
them. This repository contains none of these. openregent reads them from
an install the user owns.

Majesty is a trademark of Cyberlore Studios and/or Paradox Interactive.
openregent is not affiliated with or endorsed by either.

## Vendored code

`crates/bink` is NihAV's Bink 1 decoder, by Kostya Shishkov, trimmed to
what openregent uses.

| | |
|---|---|
| Project | NihAV, <https://nihav.org> |
| Commit | `13f31633cf325b0036d697af7bf05c611b4b8967` |
| Licence | AGPL-3.0, in [crates/bink/LICENSE](crates/bink/LICENSE) |

Each vendored file names its upstream file, author and licence at its
top. [crates/bink/README.md](crates/bink/README.md) lists the files and
the changes.

## Bundled assets

| Path | Origin | Licence |
|---|---|---|
| `crates/openregent/assets/fonts/inknut-antiqua/` | Inknut Antiqua, © 2014 Claus Eggers Sørensen | SIL Open Font License 1.1, in its `LICENSE` |

`openregent --licences` prints the font's copyright and licence, as the
OFL asks of each copy.

## Contributions

Contributions must be your own work, fit for release under AGPL-3.0. Do
not submit game files, art or text extracted from Majesty, decompiler
output, or code under a licence AGPL-3.0 cannot take. Name any outside
source you used. See [AGENTS.md](AGENTS.md) for the research rules.

A review of the repository cannot prove that every line was written
independently.
