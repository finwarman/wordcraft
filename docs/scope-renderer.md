# Scope renderer snapshot

Base: `fd0bb2fb1ec081e3eddc4d01917637fef0cafdfe` from storytold/wordcraft.

Combines current upstream with every open Fin Warman parity PR. This is an
integration snapshot, not an upstream release or a claim of full Word parity.
`convert --update-fields` is available but remains off by default.

| Upstream PR | Source commit |
|---|---|
| [#1600](https://github.com/storytold/wordcraft/pull/1600) | `7e7fcb37640ffa3dd9f55fdf45665ca75904d0a2` |
| [#1641](https://github.com/storytold/wordcraft/pull/1641) | `a2121ed48ac41c63e45395fe6b027d472e92de06` |
| [#1715](https://github.com/storytold/wordcraft/pull/1715) | `e6c28a65329a6c326b2c9ad3b07881f94cae737b` |
| [#1838](https://github.com/storytold/wordcraft/pull/1838) | `b9b722ac388b2df53173d911ee0db860d3aa97c6` |
| [#1871](https://github.com/storytold/wordcraft/pull/1871) | `f8c694848e0ad464e1e0977bc2e1d9d92cfe66f3` |
| [#1883](https://github.com/storytold/wordcraft/pull/1883) | `0dfd9ac06845c914ffabb5756b101cb642df09c2` |
| [#1931](https://github.com/storytold/wordcraft/pull/1931) | `6124586a032784374c915132ebd16bd2cad68bbf` |
| [#1990](https://github.com/storytold/wordcraft/pull/1990) | `cfdfb8840ace200c3e923b650b57619905e13804` |
| [#2000](https://github.com/storytold/wordcraft/pull/2000) | `a05637a048f968a643bfc4c38067a486e41e4f44` |
| [#2013](https://github.com/storytold/wordcraft/pull/2013) | `4b00003c5e6176729b7dea50992d7c1fe33727db` |
| [#2014](https://github.com/storytold/wordcraft/pull/2014) | `219671b13c3d47bcff4a0e3a65439bc243333167` |
| [#2047](https://github.com/storytold/wordcraft/pull/2047) | `a31c9ea55b705303c3d4a2a229cefcd6b6f61533` |

## Measured limits

The preceding local prototype matched Adobe page counts for EEMUA (60), SGS long
(59), SGS vessel (25), Rosen long (44), WSE (14), AP EEMUA (86), and Code A Weld
(3). SGS's larger fixture, Rosen approval and Rosen landscape still differ.
Matching page counts does not prove visual parity.

Remaining gaps include caption pagination, vertical table heading dimensions,
SGS photo/header overlap, Rosen approval widths and photo pagination, Code A Weld
stamp placement, cached references and EEMUA footer totals. Field refresh can
change EEMUA pagination and must not be enabled globally.

Scope supplies its fonts in its separate container. No customer documents,
Word/Adobe exports or proprietary fonts are included here. Local gate limitations
include the upstream justified-line and two colour-emoji environment failures.
