# Scope renderer snapshot

This snapshot combines upstream main `fc41918` with the parity fixes below. Scope's image pins the
integration commit SHA, never this branch's moving head. Fonts come from Scope's image build context;
no customer documents, reference PDFs or proprietary fonts are added to this repository.

| Upstream PR | Fix | Source commit |
|---|---|---|
| [#1598](https://github.com/storytold/wordcraft/pull/1598) | Layout: a table's percentage width past 100% is kept | `b5f9fd2bef867b8b6a0462782b9c42a6036da40b` |
| [#1599](https://github.com/storytold/wordcraft/pull/1599) | Shapes: a right triangle keeps its right angle | `8bba54e7ca073bcd3cd2eaa6043f7f7c98b4206b` |
| [#1600](https://github.com/storytold/wordcraft/pull/1600) | Shapes: a solid fill keeps its transparency | `67d4560eac3f0ecd106008f38c4ee744295b33b8` |
| [#1601](https://github.com/storytold/wordcraft/pull/1601) | Layout: page numbers can restart at 0 | `a455a645876e56256a9ac797942e6ef68d575780` |
| [#1611](https://github.com/storytold/wordcraft/pull/1611) | Layout: a page break followed by another page start makes one page | `4c777ba4e3f98297f909c0a2cb4fef7e2fcd804e` |
| [#1612](https://github.com/storytold/wordcraft/pull/1612) | Layout: paragraph spacing adds up when HTML auto spacing is off | `4f5e46b29fbbd6ce25fd57d2640bd3bec5b973e8` |
| [#1613](https://github.com/storytold/wordcraft/pull/1613) | Fonts: a missing roman font is drawn in Cambria, as in Word | `4005df0d0e182c923050e3c00a4ba8e5fb141061` |
| [#1614](https://github.com/storytold/wordcraft/pull/1614) | Layout: an empty section mark stays on a full page | `e59cc484b3222f5551b7c023bc4de1196ed5c3aa` |
| [#1615](https://github.com/storytold/wordcraft/pull/1615) | Layout: a bookmark after a page break takes no line | `ceab2074fe73724bcddab24b8a20e11eecc81ecf` |
| [#1616](https://github.com/storytold/wordcraft/pull/1616) | Layout: a numbered paragraph opening with a page break takes its number along | `948b51c56601ea137b557bfd6ba096c7ded94cd2` |
| [#1617](https://github.com/storytold/wordcraft/pull/1617) | Tables: text running past an exact-height row is cut off | `b37ff1f6af0f282418e911400c9132f2d8a5b0ba` |
| [#1618](https://github.com/storytold/wordcraft/pull/1618) | Tables: a row with less room than its minimum height moves on whole | `b8e801afb8fd124a2cf271de17d6b6faa7d0de85` |
| [#1619](https://github.com/storytold/wordcraft/pull/1619) | Fonts: a Bidi theme slot names the theme's complex-script font | `a66510a2bcbe66118e29c2d5e70b952a468a44a8` |
| [#1620](https://github.com/storytold/wordcraft/pull/1620) | Layout: an inherited footer sizes its percentage tables by its own section | `ddab476026b8a913ce79f2ed624eec7206de795f` |
| [#1641](https://github.com/storytold/wordcraft/pull/1641) | Tables: keep fitting exact-height rows together | `12df0b8030830d5bd6e1a14a11e3528254ab5b3a` |
| [#1644](https://github.com/storytold/wordcraft/pull/1644) | Shapes: draw flowchart OR spokes | `ae7da0bd438cb859b7c5f69c705f987a1cb512b1` |
| [#1645](https://github.com/storytold/wordcraft/pull/1645) | Draw arrowheads on open freeform paths | `96c043cc450cf5fc412964261486ac9aedfd8619` |
| [#1658](https://github.com/storytold/wordcraft/pull/1658) | Layout: empty line breaks do not move floating tables | `5a4e34dd810be6d4f9502cfaa0baf73a649ca6fd` |
| [#1665](https://github.com/storytold/wordcraft/pull/1665) | Layout: field instructions do not add blank paragraphs | `d870d32ebe9073f7c69aa56f6d4497fd04e8c0fa` |
| [#1673](https://github.com/storytold/wordcraft/pull/1673) | Layout: hide labels on page-break-only list paragraphs | `acd40c3730e4403c0e253ad35a9bb384a738840e` |

Merge resolution preserves both row-height rules, both percentage-width rules, the hidden-field
marks and every synthetic regression. Upstream status lines and revision entries are combined.

For an upgrade, fetch upstream, rebuild this snapshot from the remaining unmerged fix PRs, run
`cargo xtask ci`, compare the generated reports with Adobe, then update the Scope image's commit pin.
The snapshot adds no local rendering features outside these PRs.

Passing page counts do not establish pixel parity. Remaining corpus drift and unverified annotation,
formula and image cases must be measured before expanding the opt-in renderer.
