# Scope renderer snapshot

Upstream base: `eae242176108d328c5dd81fbc10370c51ab90706`.
Previous Scope snapshot: `44934d06375ce4a43f5e1bdc5c20c04a0f1c100e`.

This integrates upstream and the parity contributions below. It is an alpha
snapshot, not an upstream release or a claim of complete Word parity. Rust 1.95
is required. Scope supplies fonts in its separate container.

Native controls use Microsoft Word for Mac 16.113.2, build 26092012, with local
Best for printing export. Adobe PDFs are secondary baselines; their rendering
engine and version are not established.

## Contributions

PR heads identify the related source contributions. Rebases can change these
commits without changing the integrated rules. Fragment dependencies are
#2465, #2493, #2495, then #2498.

| PR | Source head |
|---|---|
| [#1600](https://github.com/storytold/wordcraft/pull/1600) | `1cd50e6416daffe9aa0b6a61a312b83920e6627d` |
| [#1641](https://github.com/storytold/wordcraft/pull/1641) | `71839a9c24cfad12040221cb72bc9d80dfa09f76` |
| [#1715](https://github.com/storytold/wordcraft/pull/1715) | `8cc6b9f469b3a7c5671a50f17cbc4c06cb663492` |
| [#1838](https://github.com/storytold/wordcraft/pull/1838) | `198a4ffa0d298abf24d34bfb43787efcedd602e8` |
| [#1871](https://github.com/storytold/wordcraft/pull/1871) | `4edb458343b6b8adc3779e1a29963a926145b579` |
| [#1883](https://github.com/storytold/wordcraft/pull/1883) | `59c7ae38c88a92955dea525bc41b5b1dc0a220a1` |
| [#1931](https://github.com/storytold/wordcraft/pull/1931) | `91183d880780f87492762e82d8774bc29d5865ea` |
| [#1990](https://github.com/storytold/wordcraft/pull/1990) | `cfdfb8840ace200c3e923b650b57619905e13804` |
| [#2000](https://github.com/storytold/wordcraft/pull/2000) | `a05637a048f968a643bfc4c38067a486e41e4f44` |
| [#2013](https://github.com/storytold/wordcraft/pull/2013) | `4b00003c5e6176729b7dea50992d7c1fe33727db` |
| [#2014](https://github.com/storytold/wordcraft/pull/2014) | `219671b13c3d47bcff4a0e3a65439bc243333167` |
| [#2047](https://github.com/storytold/wordcraft/pull/2047) | `a31c9ea55b705303c3d4a2a229cefcd6b6f61533` |
| [#2117](https://github.com/storytold/wordcraft/pull/2117) | `0a637f0a8e6e23f919f9d91b67ecb42a424ea593` |
| [#2431](https://github.com/storytold/wordcraft/pull/2431) | `425e01dec9de10e29d371bd0cf64f3bed57e5d9c` |
| [#2432](https://github.com/storytold/wordcraft/pull/2432) | `d55eae947cf3db3cd2a46ad4a2f59c180c08206f` |
| [#2433](https://github.com/storytold/wordcraft/pull/2433) | `99175a5410d34286af0f8cd2f073f3c0c5fdd8fc` |
| [#2434](https://github.com/storytold/wordcraft/pull/2434) | `087b9e78584389e8cdbe29570e9371f5f6435dbb` |
| [#2435](https://github.com/storytold/wordcraft/pull/2435) | `84563223526810c2a59fde6e8ec37894eda91f34` |
| [#2436](https://github.com/storytold/wordcraft/pull/2436) | `eade6fd736c9efce36e5a33f51d8df1d886a9438` |
| [#2437](https://github.com/storytold/wordcraft/pull/2437) | `2112fc760909048a3a506a9f05ebca5a60205248` |
| [#2453](https://github.com/storytold/wordcraft/pull/2453) | `3e2cf5edb445111d84495d139f8e3e6957124411` |
| [#2454](https://github.com/storytold/wordcraft/pull/2454) | `f637c1ac88f6be08b2e7995971e17684ecda69a9` |
| [#2459](https://github.com/storytold/wordcraft/pull/2459) | `109f3e02127857d7bce2f01caee303ef9ccb7341` |
| [#2461](https://github.com/storytold/wordcraft/pull/2461) | `19e0877ed2207454996e649fc8201323a7433f25` |
| [#2462](https://github.com/storytold/wordcraft/pull/2462) | `07e902af8cac21617b215dab017f4ebe95195a43` |
| [#2464](https://github.com/storytold/wordcraft/pull/2464) | `53a6bfb715d33f4118fe3534989dc994a267c436` |
| [#2465](https://github.com/storytold/wordcraft/pull/2465) | `ebdc259aac4091b2936305168a01bd1511783775` |
| [#2466](https://github.com/storytold/wordcraft/pull/2466) | `d6b2dec4ef7a5367bdd5cc5396a9f14c556aed67` |
| [#2469](https://github.com/storytold/wordcraft/pull/2469) | `802dc45db318c71e3d80fcb2d09ce0481d843340` |
| [#2473](https://github.com/storytold/wordcraft/pull/2473) | `0c23d923bf231be24cdf008e0cce0a0f1424806e` |
| [#2475](https://github.com/storytold/wordcraft/pull/2475) | `067e96c90b1b47747c9a746891ac9656cfdd2048` |
| [#2482](https://github.com/storytold/wordcraft/pull/2482) | `3c1006f53681d3243ae35bb5bae880d3b03ae6bd` |
| [#2484](https://github.com/storytold/wordcraft/pull/2484) | `e56b0761ebc21e899694b9916d4ec9a6e9ee9d4a` |
| [#2485](https://github.com/storytold/wordcraft/pull/2485) | `e33a2c469db95a8fe4400d551afc29215450eff0` |
| [#2487](https://github.com/storytold/wordcraft/pull/2487) | `a45675f0ba0bf9eee1210129d143506ad3188a57` |
| [#2493](https://github.com/storytold/wordcraft/pull/2493) | `ac00e9faabd14fffe9476d27ea4fb447f2cade78` |
| [#2494](https://github.com/storytold/wordcraft/pull/2494) | `85b6a1f42ba7133351c3c736eac07b8b7521cc8c` |
| [#2495](https://github.com/storytold/wordcraft/pull/2495) | `77c4906f15fb25fd755c451cad0030788f259806` |
| [#2498](https://github.com/storytold/wordcraft/pull/2498) | `a9bbdfbe8e5139fcc1c964a15370d444e99b6366` |
| [#2500](https://github.com/storytold/wordcraft/pull/2500) | `09f065c11f98f3c5c234beec84e17805842749fb` |

## Measured limits

The local ten-report corpus retains frozen Adobe page counts, landscape page
indices and ordered figure/table labels, including repeated instances. Nine
full native Word exports also match physical page counts. Word exits while
opening the SGS long-report copy; that case is not certified against native
Word. Equal counts do not establish visual or semantic parity.

Normal conversion retains cached TOC entries. Explicit `--update-fields` is a
separate workflow. It preserves number tabs and table-cell headings. Native
Word's Update Table confirms SGS's eleven regenerated entries and repaired
appendix reference. EEMUA's 53 regenerated entries and six corrected PDF outline
destinations reach their final headings. The Scope wrapper does not enable
field refresh globally.

Remaining observations: EEMUA appendix headings are about 18 pt lower and use
one line in native Word, while this snapshot and Adobe use two lines. The rule
is still under investigation. A Rosen final table continuation differs by about
1.8 pt. Neither is hidden by the page-count checks.

Local gates encounter the reproduced upstream justified-line and two
colour-emoji environment failures. Those tests are unchanged. No customer
documents, Word/Adobe exports or proprietary fonts are included here.
