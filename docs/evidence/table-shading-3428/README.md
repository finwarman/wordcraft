# Table paragraph shading

These synthetic page renders compare main `b70c746f` with renderer code at `1051bb2f`.
The example is recorded separately by hash. Linux ARM64, Rust 1.99, bundled Source Sans 3,
400 x 200 pt pages, 2 px per pt. No Word-generated files or customer inputs are included.

Reproduce at either source revision with the same example:

```sh
cargo run -p wordcraft-render --example table_shading -- /tmp/table-shading
```

| Model/page | Before | After |
|---|---|---|
| spacing-1 | ![Before spacing-1](before-spacing-1.png) | ![After spacing-1](after-spacing-1.png) |
| spacing-2 | ![Before spacing-2](before-spacing-2.png) | ![After spacing-2](after-spacing-2.png) |
| widow-1 | ![Before widow-1](before-widow-1.png) | ![After widow-1](after-widow-1.png) |
| widow-2 | ![Before widow-2](before-widow-2.png) | ![After widow-2](after-widow-2.png) |
| partial-2 | ![Before partial-2](before-partial-2.png) | ![After partial-2](after-partial-2.png) |
| closing-2 | ![Before closing-2](before-closing-2.png) | ![After closing-2](after-closing-2.png) |
| cell-2 | ![Before cell-2](before-cell-2.png) | ![After cell-2](after-cell-2.png) |
| equal-2 | ![Before equal-2](before-equal-2.png) | ![After equal-2](after-equal-2.png) |

Spacing: shading leaves page 1 with its paragraph and follows it to page 2.
Widow: the kept shaded paragraph moves together. Partial: each continuation fill follows
its own line fragment. Closing: shading clears the cell margin and bottom rule.
Cell backgrounds and equal-colour cell/paragraph controls retain all three page PNGs
byte-for-byte. Only representative control pages are included here.

[renders.json](renders.json) binds the source, model, font and included PNGs.
This is paint evidence. It does not establish new native Word or general table parity.
