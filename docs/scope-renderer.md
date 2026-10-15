# Scope renderer snapshot

Base: `45709520696db6ae6287e16f42f5c15fd4249c39` from storytold/wordcraft.

This fork snapshot integrates the patches below with the base's newer layout fixes.
Upstream now includes #1598, #1599 and #1601. The VML fixture expectation also follows upstream fix `3b16fac`. The remaining source regressions are retained. Shared documentation conflicts are consolidated here.
It is an integration build, not an upstream release or a claim of full Word parity.

| Upstream PR | Source commit |
|---|---|
| [#1715](https://github.com/storytold/wordcraft/pull/1715) | `e6c28a65329a6c326b2c9ad3b07881f94cae737b` |
| [#1709](https://github.com/storytold/wordcraft/pull/1709) | `20a102e42e17495c8d23c4849ae9d9a57496da73` |
| [#1708](https://github.com/storytold/wordcraft/pull/1708) | `268c811cf900516e8a2b6a5280ee2ac3590f56dc` |
| [#1708](https://github.com/storytold/wordcraft/pull/1708) | `90d8020d5a79136e139298e27fd1c556e4cca772` |
| [#1673](https://github.com/storytold/wordcraft/pull/1673) | `4d630fd6d3f996c44a99483c2bc98bfad398de78` |
| [#1665](https://github.com/storytold/wordcraft/pull/1665) | `2ac9b3c87d272601847732e3fb44b6fc32ca7383` |
| [#1658](https://github.com/storytold/wordcraft/pull/1658) | `261041c08223f3a2fba80db4ea65c1c2c6c404ee` |
| [#1645](https://github.com/storytold/wordcraft/pull/1645) | `1807c95404738b01536963dbaa4702d394bbf06e` |
| [#1644](https://github.com/storytold/wordcraft/pull/1644) | `413f0f8f9f5d9a2ada15fe452a827b02c1f9d7f1` |
| [#1641](https://github.com/storytold/wordcraft/pull/1641) | `a2121ed48ac41c63e45395fe6b027d472e92de06` |
| [#1620](https://github.com/storytold/wordcraft/pull/1620) | `eb1db72caa4c45c1cb080389578c75ebb27ca0f2` |
| [#1619](https://github.com/storytold/wordcraft/pull/1619) | `b6b180001bd42ec7243a9c77ccf6aefd39d8a8bd` |
| [#1618](https://github.com/storytold/wordcraft/pull/1618) | `d366b3b5d55ef457010946f8b4eceea554e5e956` |
| [#1618](https://github.com/storytold/wordcraft/pull/1618) | `d00f34879285c9a83df99431196807eb3807cac1` |
| [#1617](https://github.com/storytold/wordcraft/pull/1617) | `41f0d3ba3adde561fc67d6ab389fd1eb4f42eaa7` |
| [#1617](https://github.com/storytold/wordcraft/pull/1617) | `257e65fccc44877a1240d4e86073625021a3e9b7` |
| [#1616](https://github.com/storytold/wordcraft/pull/1616) | `2441551c4a3b7f4265a40466d470ec491d246c2e` |
| [#1615](https://github.com/storytold/wordcraft/pull/1615) | `5dee1e37f05d2d168b0ceb7282c52c81208a85b1` |
| [#1614](https://github.com/storytold/wordcraft/pull/1614) | `5de4b283215d83a7f38f35ded13a7bbeca675c04` |
| [#1614](https://github.com/storytold/wordcraft/pull/1614) | `a693ebef41443e611bf35e5747471e752bb95a10` |
| [#1613](https://github.com/storytold/wordcraft/pull/1613) | `232de1c03799848a1fb173cae4087be83311a468` |
| [#1612](https://github.com/storytold/wordcraft/pull/1612) | `2cb57d363e9aa71994a6abfcd865a44a78f99487` |
| [#1611](https://github.com/storytold/wordcraft/pull/1611) | `02710bfcf53cbd0fe694f025345510c6cfe31419` |
| [#1601](https://github.com/storytold/wordcraft/pull/1601) | `3d4d3c59ef2aff43f71c7fe0e9f2fae4b24b7023` |
| [#1600](https://github.com/storytold/wordcraft/pull/1600) | `7e7fcb37640ffa3dd9f55fdf45665ca75904d0a2` |
| [#1599](https://github.com/storytold/wordcraft/pull/1599) | `a2d21169a45b79a6de263ff13bee28ab5d4d3fb9` |
| [#1598](https://github.com/storytold/wordcraft/pull/1598) | `695e05326628ca6e2229dc7d368e64f24d0ffd29` |
| [#1598](https://github.com/storytold/wordcraft/pull/1598) | `fa97ab0c2803bb0e305993c4a409cca0d78a05a0` |
| [#1838](https://github.com/storytold/wordcraft/pull/1838) | `6e6d04f55bbf321057f6e6084e097dbd7f000fbd` |
| [#1871](https://github.com/storytold/wordcraft/pull/1871) | `f8bbf45df6cfb4370fe56ec0679f298b52fde87c` |
| [#1883](https://github.com/storytold/wordcraft/pull/1883) | `1316890f7fd05a67d679e7b78a1fc63dfc4fa705` |

## Remaining comparison gaps

- EEMUA footer totals differ from both Adobe and native Word. Behind-text blank frames now align; broader image positioning still needs comparison.
- SGS preferred columns no longer equalize under spanning images. Residual width, row height and pagination drift remains.
- Rosen photo layout and pagination need a new corpus render.
- Adobe and native Word differ on spacing after manual page breaks and caption pagination.
- Scope’s synthetic signature JPEGs were repaired. Code A Weld stamp and signature placement still differ.

Scope supplies its font bundle in its separate container build. No customer documents,
Word exports, Adobe exports or proprietary fonts are included in this snapshot.

The local gate has three confirmed upstream environment failures: the justified-line
font comparison and both colour-emoji tests. The remaining gate runs with those
three tests excluded; these exclusions do not change source behavior.
