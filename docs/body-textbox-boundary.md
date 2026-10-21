# Wrapping body textboxes at page boundaries

> **Last updated:** 2026-10-21 · **Change:** trivial (bounded body-textbox placement and wrap replay, #2731).

In Print view, unturned wrapping textboxes anchored in top-level body paragraphs use their fitted frame before placement. Compatibility modes below 15 keep fitting frames within the physical page. In modern modes, fitting Paragraph/Line anchors stay between the text area's top and bottom margins. Modern Page/Margin anchors retain absolute placement and may overflow.

A modern Paragraph/Line frame taller than the text area starts after its anchor line, on a fresh column when needed. It keeps its full frame height. Oversized wrapping and paragraph continuation retain the split policy; its cache key stays separate from ordinary wrapping. Non-textbox floats, turned boxes, inline and Front/Behind objects retain their existing behavior.

If a corrected frame reaches earlier body text, bounded replay supplies that wrap area before the text is laid out again. Replay is capped at four retries. This does not establish convergence or a memory bound. Public paragraph callers retain their existing API and ordinary long-word behavior. Removing alignment keeps page-fixed drawings page-fixed.

Original model/import controls and private native exports cover representative legacy/modern anchoring, fitted frames, oversized placement and backward wrapping. Three known omitted-spacing frame offsets remain about 0.1221 pt. The inherited TallInline height difference is about +1.280857 pt, with a -0.5 pt image-origin offset on each axis. These are explicit limits, not exact native geometry or raster parity.

The reader, source markers, hit/line ownership, descendant own-paint policy and static outer fit-cache contract remain those of [textbox-float-policy.md](textbox-float-policy.md). No broader nested-descendant rule is added.

## Revision history

| Date | Change |
|---|---|
| 2026-10-21 | Bounded body textbox placement and backward wrap replay; inherited native geometry limits retained. |
