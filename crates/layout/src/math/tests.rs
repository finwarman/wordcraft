use super::*;
use wordcraft_doc::math::Math;

fn rc() -> ResolvedChar {
    wordcraft_doc::styles::StyleSheet::default().resolve_char(None, &Default::default())
}

fn lay(lin: &str, display: bool) -> MathLayout {
    layout_equation(&Math::default(), lin, &rc(), display, 400.0, &MathProps::default(), &mut 0)
}

/// The glyphs of a laid-out equation by line: (baseline, [(x, text)]), top line first.
fn lines(m: &MathLayout) -> Vec<(f32, Vec<(f32, String)>)> {
    let mut out: Vec<(f32, Vec<(f32, String)>)> = Vec::new();
    for it in &m.items {
        if let MItem::Glyphs { glyphs, text, .. } = it
            && let Some(&(_, x, y)) = glyphs.first()
        {
            match out.iter_mut().find(|l| (l.0 - y).abs() < 0.5) {
                Some(l) => l.1.push((x, text.clone())),
                None => out.push((y, vec![(x, text.clone())])),
            }
        }
    }
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    for l in &mut out {
        l.1.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
    out
}

fn lay_in(math: &Math, lin: &str, display: bool, avail: f32, mp: &MathProps) -> MathLayout {
    layout_equation(math, lin, &rc(), display, avail, mp, &mut 0)
}

#[test]
fn eq_array_rows_without_ampersand_are_centred() {
    // Word centres a row with no `&` in the array (#2617: a case row "a, n odd" above
    // "|a|, &n even"), whether or not the other rows have one.
    use wordcraft_doc::math::*;
    let run = |s: &str| vec![MNode::Run(MRun::new(s))];
    let w_a = lay("a", true).width;
    for other in ["bbbbbb&c", "bbbbbbc"] {
        let math = Math { nodes: vec![MNode::EqArr { rows: vec![run("a"), run(other)] }], ..Default::default() };
        let m = lay_in(&math, "", true, 400.0, &MathProps::default());
        let ls = lines(&m);
        let xa = ls.first().and_then(|l| l.1.first()).map(|g| g.0).unwrap_or(f32::NAN);
        let x0 = ls.get(1).and_then(|l| l.1.first()).map(|g| g.0).unwrap_or(f32::NAN);
        let want = x0 + (m.width - x0 - w_a) / 2.0;
        assert!((xa - want).abs() < 1.0, "{other}: a at {xa}, want {want} ({ls:?}, width {})", m.width);
        // The lone row doesn't widen the aligned columns.
        assert!(x0.abs() < 0.5 && m.width < lay("bbbbbbc", true).width + 1.0, "{other}: {ls:?}, width {}", m.width);
    }
    // A lone row wider than the aligned columns sets the array's width; the aligned
    // rows are centred under it as one block.
    let math = Math { nodes: vec![MNode::EqArr { rows: vec![run("bbbbbbbbbb"), run("a&c")] }], ..Default::default() };
    let m = lay_in(&math, "", true, 400.0, &MathProps::default());
    let ls = lines(&m);
    let xw = ls.first().and_then(|l| l.1.first()).map(|g| g.0).unwrap_or(f32::NAN);
    let x0 = ls.get(1).and_then(|l| l.1.first()).map(|g| g.0).unwrap_or(f32::NAN);
    assert!(xw.abs() < 0.5 && (m.width - lay("bbbbbbbbbb", true).width).abs() < 1.0, "wide lone row: {ls:?}, width {}", m.width);
    let want = (m.width - w_a - lay("c", true).width) / 2.0;
    assert!(want > 1.0 && (x0 - want).abs() < 1.0, "aligned row at {x0}, want {want} ({ls:?})");
}

#[test]
fn long_display_equations_wrap_before_operators_and_indent() {
    let sum = "y=a+b+c+d+e+f+g+h+i+j+k+l+m+n+o+p+q+r+s+t+u+v+w";
    let mp = MathProps::default();
    let full = lay_in(&Math::default(), sum, true, 1e4, &mp);
    assert_eq!(lines(&full).len(), 1, "fits on one line when there is room");
    let avail = full.width * 0.45;
    assert!(avail > 2.0 * mp.wrap_indent, "room for the indent: {avail}");
    let m = lay_in(&Math::default(), sum, true, avail, &mp);
    let ls = lines(&m);
    assert!(ls.len() >= 2, "{ls:?}");
    assert!((m.width - avail).abs() < 0.01, "the wrapped equation fills the line: {} vs {avail}", m.width);
    for (k, (_, gs)) in ls.iter().enumerate() {
        let (x0, first) = gs.first().cloned().unwrap();
        if k == 0 {
            // Math italic 𝑦 when a font has it; plain y where none does (FreeBSD CI has no fonts).
            assert!(x0.abs() < 0.01 && (first == "𝑦" || first == "y"), "{gs:?}");
        } else {
            // Continuation lines start with the operator, one wrap indent in.
            assert!((x0 - mp.wrap_indent).abs() < 0.01 && first == "+", "line {k}: {gs:?}");
        }
        for (x, t) in gs {
            assert!(*x < avail, "line {k}: {t} at {x} past {avail}");
        }
    }
    // Breaks after the operator, and the operator repeated, as the settings say.
    let after = MathProps { brk_bin: BrkBin::After, ..Default::default() };
    let ls = lines(&lay_in(&Math::default(), sum, true, avail, &after));
    assert!(ls.len() >= 2 && ls[0].1.last().is_some_and(|g| g.1 == "+") && ls[1].1[0].1 != "+", "{ls:?}");
    let repeat = MathProps { brk_bin: BrkBin::Repeat, wrap_right: true, ..Default::default() };
    let m = lay_in(&Math::default(), sum, true, avail, &repeat);
    let ls = lines(&m);
    assert!(ls.len() >= 2 && ls[0].1.last().is_some_and(|g| g.1 == "+") && ls[1].1[0].1 == "+", "{ls:?}");
    // Inline equations never wrap.
    assert_eq!(lines(&lay_in(&Math::default(), sum, false, avail, &mp)).len(), 1);
    // A term wider than the line overflows it rather than breaking inside.
    let frac = lay_in(&Math::default(), "y=(a+b+c+d+e+f+g+h+i+j+k+l+m+n+o+p+q+r+s+t+u+v+w)/2", true, avail, &mp);
    assert!(frac.width > avail && frac.items.iter().any(|i| matches!(i, MItem::Rect { .. })), "{}", frac.width);
}

#[test]
fn manual_breaks_start_new_lines() {
    let mp = MathProps::default();
    let nodes = vec![MNode::Run(MRun::new("a=b")), MNode::Run(MRun { brk: Some(0), ..MRun::new("+c") })];
    let m = lay_in(&Math { nodes, ..Default::default() }, "", true, 400.0, &mp);
    let ls = lines(&m);
    assert_eq!(ls.len(), 2, "{ls:?}");
    assert!((ls[1].1[0].0 - mp.wrap_indent).abs() < 0.01 && ls[1].1[0].1 == "+", "{ls:?}");
    assert!(m.width < 400.0, "a manually broken equation is as wide as its lines");
    // `m:alnAt`: the new line's operator sits under the first line's first operator.
    let nodes = vec![MNode::Run(MRun::new("a=b")), MNode::Run(MRun { brk: Some(1), ..MRun::new("+c") })];
    let ls = lines(&lay_in(&Math { nodes, ..Default::default() }, "", true, 400.0, &mp));
    let eq_x = ls[0].1.iter().find(|g| g.1 == "=").unwrap().0;
    assert!((ls[1].1[0].0 - eq_x).abs() < 0.01, "{ls:?}");
}

#[test]
fn short_display_equations_stay_on_one_line() {
    let mp = MathProps::default();
    let one = lay_in(&Math::default(), "a+b=c", true, 400.0, &mp);
    assert_eq!(lines(&one).len(), 1);
    let plain = lay_in(&Math::default(), "a+b=c", false, 400.0, &mp);
    assert!(one.width < 400.0 && (one.width - plain.width).abs() < 0.01, "{} vs {}", one.width, plain.width);
}

fn script_kern_bytes(base: u16, script: u16) -> Vec<u8> {
    let gi = 10 + 8 + (C::RadicalKernAfterDegree as usize + 1) * 4 + 2;
    let ki = gi + 8;
    let mut bytes = vec![0u8; ki + 40];
    let mut put = |at, value: u16| bytes[at..at + 2].copy_from_slice(&value.to_be_bytes());
    put(0, 1);
    put(4, 10);
    put(6, gi as u16);
    put(10, 73);
    put(12, 60);
    put(gi + 6, 8);
    put(ki, 20);
    put(ki + 2, 2);
    let mut glyphs = [base, script];
    glyphs.sort_unstable();
    for (i, glyph) in glyphs.into_iter().enumerate() {
        let corner = if glyph == base { 2 } else { 1 };
        put(ki + 4 + i * 8 + corner * 2, if glyph == base { 28 } else { 34 });
        put(ki + 24 + i * 2, glyph);
    }
    put(ki + 20, 1);
    put(ki + 22, 2);
    put(ki + 30, (-100i16) as u16);
    put(ki + 36, (-50i16) as u16);
    bytes
}

#[test]
fn subscript_math_kerning() {
    let db = FontDb::with_font_dirs(Vec::new());
    db.set_system_fallback(false);
    let font = db.face(wordcraft_fonts::DEFAULT_FAMILY, "Regular");
    let face = FaceRef::of(&font);
    let bytes = script_kern_bytes(face.glyph_for('V') as u16, face.glyph_for('a') as u16);
    for size in [9.0, 14.0, 20.0] {
        let mut cx = Ctx {
            face,
            t: Some(Arc::new(wordcraft_fonts::math::parse(&bytes).unwrap())),
            upem: face.upem as f32,
            base: size,
            color: Rgb::BLACK,
            text_font: wordcraft_fonts::DEFAULT_FAMILY.into(),
            nodes: 0,
            path: Vec::new(),
            node_stack: Vec::new(),
        };
        let st = St { level: 0, display: true, fraction_size: FracSize::Text, frac_edges: FracEdges::Spaced, cramped: false, upright: false };
        for ratio in [1.0, 1.4] {
            let base_size = size * ratio;
            let base = cx.glyphs(face, "V", base_size, 0, Rgb::BLACK, (false, false));
            let script_size = cx.size(st.script());
            let sub = cx.glyphs(face, "a", script_size, 1, Rgb::BLACK, (false, false));
            let sub_x = |b: &super::bx::Bx| {
                b.items
                    .iter()
                    .find_map(|i| match i {
                        MItem::Glyphs { glyphs, text, .. } if text == "a" => glyphs.first().map(|g| g.1),
                        _ => None,
                    })
                    .unwrap()
            };
            for boxed_script in [false, true] {
                let mut sub = sub.clone();
                if boxed_script {
                    sub.glyph = None;
                }
                let result = cx.attach(base.clone(), Some(sub), None, st, true);
                let kern = if ratio == 1.0 { (-100.0 * base_size - if boxed_script { 0.0 } else { 50.0 * script_size }) / cx.upem } else { 0.0 };
                assert!((sub_x(&result) - (base.w + kern).max(0.0)).abs() < 0.001, "font corner kerning positions the subscript");
            }
            let mut boxed_base = base.clone();
            boxed_base.glyph = None;
            let result = cx.attach(boxed_base, Some(sub), None, st, false);
            assert_eq!(sub_x(&result), base.w, "a compound base keeps its existing script attachment");
        }
        let mut stepped = bytes.clone();
        let gi = u16::from_be_bytes([stepped[6], stepped[7]]) as usize;
        let ki = gi + 8;
        stepped.resize(ki + 56, 0);
        let mut put = |at, value: i16| stepped[at..at + 2].copy_from_slice(&value.to_be_bytes());
        let mut glyphs = [face.glyph_for('V') as u16, face.glyph_for('a') as u16];
        glyphs.sort_unstable();
        let script_index = glyphs.iter().position(|g| *g == face.glyph_for('a') as u16).unwrap();
        put(ki + 4 + script_index * 8 + 2, 50);
        put(ki + 28, 2);
        for (i, value) in [0i16, 300, -100, -300, 50].into_iter().enumerate() {
            put(ki + 30 + i * 4, value);
        }
        put(ki + 50, 0);
        put(ki + 52, -50);
        cx.t = Some(Arc::new(wordcraft_fonts::math::parse(&stepped).unwrap()));
        let scale = size / cx.upem;
        let mut base = cx.glyphs(face, "V", size, 0, Rgb::BLACK, (false, false));
        base.d = 200.0 * scale;
        let mut sub = cx.glyphs(face, "a", cx.size(st.script()), 1, Rgb::BLACK, (false, false));
        sub.a = 600.0 * scale;
        for (drop, expected) in [(200.0, -100.0), (500.0, -300.0)] {
            let kern = cx.sub_kern(&base, &sub, drop * scale);
            let expected = expected * scale - 50.0 * cx.size(st.script()) / cx.upem;
            assert!((kern - expected).abs() < 0.001, "choose the smaller sum at the two touching heights");
        }
    }
}

#[test]
fn enclosed_fraction_edges() {
    let db = FontDb::with_font_dirs(Vec::new());
    db.set_system_fallback(false);
    let font = db.face(wordcraft_fonts::DEFAULT_FAMILY, "Regular");
    let face = FaceRef::of(&font);
    for display in [false, true] {
        for size in [9.0, 14.0, 20.0] {
            let mut cx = Ctx {
                face,
                t: None,
                upem: face.upem as f32,
                base: size,
                color: Rgb::BLACK,
                text_font: wordcraft_fonts::DEFAULT_FAMILY.into(),
                nodes: 0,
                path: Vec::new(),
                node_stack: Vec::new(),
            };
            let st = St { level: 0, display, fraction_size: FracSize::Text, frac_edges: FracEdges::Spaced, cramped: false, upright: false };
            let frac = MNode::Frac { kind: FracKind::Bar, num: vec![MNode::Run(MRun::new("a"))], den: vec![MNode::Run(MRun::new("b+1"))] };
            let rad = MNode::Rad { deg: Vec::new(), deg_hide: true, e: vec![frac.clone()] };
            let result = cx.node(&rad, st, 0);
            let bars: Vec<_> = result
                .items
                .iter()
                .filter_map(|i| match i {
                    MItem::Rect { x, w, .. } => Some((x, w)),
                    _ => None,
                })
                .collect();
            assert_eq!(bars.len(), 2);
            assert!((bars[0].0 - bars[1].0).abs() < 0.001, "radical and fraction start together: {bars:?}");
            assert!((bars[0].1 - bars[1].1).abs() < 0.001, "radical and fraction end together: {bars:?}");

            let nested = MNode::Frac { kind: FracKind::Bar, num: vec![frac.clone()], den: vec![MNode::Run(MRun::new("c"))] };
            let plain = cx.node(&nested, st, 0);
            let inside = cx.node(&MNode::Rad { deg: Vec::new(), deg_hide: true, e: vec![nested] }, st, 0);
            let rule_width = |b: &super::bx::Bx, index| {
                b.items
                    .iter()
                    .filter_map(|i| match i {
                        MItem::Rect { w, .. } => Some(*w),
                        _ => None,
                    })
                    .nth(index)
                    .unwrap()
            };
            assert_eq!(rule_width(&inside, 1), rule_width(&plain, 0), "nested fraction spacing stays unchanged");

            for body in [
                vec![MNode::Run(MRun::new("(")), frac.clone(), MNode::Run(MRun::new(")"))],
                vec![MNode::Delim { beg: Some('('), end: Some(')'), sep: None, grow: false, shp_match: false, elems: vec![vec![frac.clone()]] }],
            ] {
                let result = cx.node(&MNode::Rad { deg: Vec::new(), deg_hide: true, e: body }, st, 0);
                let (bar_x, bar_w) = result
                    .items
                    .iter()
                    .filter_map(|i| match i {
                        MItem::Rect { x, w, .. } => Some((x, w)),
                        _ => None,
                    })
                    .nth(1)
                    .unwrap();
                let (left, right) = ('(', ')');
                let glyph_x = |c: char| {
                    result
                        .items
                        .iter()
                        .find_map(|i| match i {
                            MItem::Glyphs { glyphs, text, .. } if *text == c.to_string() => glyphs.first().map(|g| g.1),
                            _ => None,
                        })
                        .unwrap()
                };
                let left_end = glyph_x(left) + face.advance(face.glyph_for(left)) as f32 * size / face.upem as f32;
                assert!((bar_x - left_end).abs() < 0.001, "no fraction margin after opening bracket");
                assert!((bar_x + bar_w - glyph_x(right)).abs() < 0.001, "no fraction margin before closing bracket");
            }
        }
    }
}

#[test]
fn fraction_is_stacked_with_a_rule() {
    let flat = lay("ab", true);
    let f = lay("a/b", true);
    assert!(f.ascent > flat.ascent + 2.0 && f.descent > flat.descent + 2.0, "{f:?}");
    assert!(f.items.iter().any(|i| matches!(i, MItem::Rect { .. })), "fraction rule");
    assert!(f.width < flat.width * 1.2, "stacked, not side by side");
}

#[test]
fn superscript_is_raised_and_smaller() {
    let m = lay("x^2", false);
    let sizes: Vec<(f32, f32)> = m
        .items
        .iter()
        .filter_map(|i| match i {
            MItem::Glyphs { size, glyphs, .. } => glyphs.first().map(|g| (*size, g.2)),
            _ => None,
        })
        .collect();
    assert_eq!(sizes.len(), 2, "{m:?}");
    assert!(sizes[1].0 < sizes[0].0 && sizes[1].1 > 2.0, "{sizes:?}");
}

#[test]
fn operators_get_spaces_and_unary_minus_does_not() {
    let tight = lay("ab", false);
    let spaced = lay("a+b", false);
    let unary = lay("−b", false);
    let plus_w = lay("+", false).width;
    assert!(spaced.width > tight.width + plus_w + 2.0, "medium spaces around +");
    assert!(unary.width < lay("b", false).width + plus_w + 1.0, "no space after a unary minus");
}

#[test]
fn math_leading_is_not_ink() {
    let db = FontDb::with_font_dirs(Vec::new());
    db.set_system_fallback(false);
    let font = db.face(wordcraft_fonts::DEFAULT_FAMILY, "Regular");
    let face = FaceRef::of(&font);
    for units in [None, Some(0i16), Some(200), Some(-200)] {
        let table = units.map(|value| {
            // Original minimal MATH header and constants, with no glyph-specific data.
            let mut bytes = vec![0u8; 22];
            bytes[0..2].copy_from_slice(&1u16.to_be_bytes());
            bytes[4..6].copy_from_slice(&10u16.to_be_bytes());
            bytes[18..20].copy_from_slice(&value.to_be_bytes());
            Arc::new(wordcraft_fonts::math::parse(&bytes).unwrap())
        });
        for display in [false, true] {
            for size in [9.0, 14.0, 20.0] {
                let cx = Ctx {
                    face,
                    t: table.clone(),
                    upem: face.upem as f32,
                    base: size,
                    color: Rgb::BLACK,
                    text_font: wordcraft_fonts::DEFAULT_FAMILY.into(),
                    nodes: 0,
                    path: Vec::new(),
                    node_stack: Vec::new(),
                };
                let st = St {
                    level: 0,
                    display,
                    fraction_size: if display { FracSize::Text } else { FracSize::Script },
                    frac_edges: FracEdges::Spaced,
                    cramped: false,
                    upright: false,
                };
                let mut ink = cx.glyph_id(face.glyph_for('a'), size, Rgb::BLACK);
                ink.items.push(MItem::Rect { x: 0.0, y: 0.0, w: ink.w, h: 0.5, color: Rgb::BLACK });
                let before = ink.clone();
                let result = cx.finish(ink, st);
                let leading = f32::from(units.unwrap_or(0).max(0)) * size / cx.upem;
                assert!((result.ascent - before.a - leading).abs() < 0.001, "reserve only the font's nonnegative leading");
                assert_eq!(result.descent, before.d);
                assert_eq!(result.width, before.w);
                for (actual, expected) in result.items.iter().zip(&before.items) {
                    match (actual, expected) {
                        (MItem::Glyphs { glyphs: a, size: sa, .. }, MItem::Glyphs { glyphs: b, size: sb, .. }) => assert_eq!((sa, a), (sb, b)),
                        (MItem::Rect { x, y, w, h, .. }, MItem::Rect { x: bx, y: by, w: bw, h: bh, .. }) => {
                            assert_eq!((x, y, w, h), (bx, by, bw, bh));
                        }
                        _ => panic!("drawing item changed"),
                    }
                }
                assert_eq!(result.items.len(), before.items.len());
            }
        }
    }
}

#[test]
fn display_script_fraction_size() {
    let st = St { level: 0, display: true, fraction_size: FracSize::Text, frac_edges: FracEdges::Spaced, cramped: false, upright: false };
    assert_eq!(st.script().frac().level, 1, "display script fractions retain their argument size");
    let tight = St { frac_edges: FracEdges::Tight, ..st };
    assert_eq!(tight.frac().frac_edges, FracEdges::Spaced, "nested fraction arguments keep their spacing");
    assert_eq!(tight.script().frac_edges, FracEdges::Spaced, "scripts keep their own spacing");
    let small = St { fraction_size: FracSize::Display, ..st };
    assert_eq!(small.frac().level, 0, "the outer display fraction stays full size");
    assert_eq!(small.frac().frac().level, 1, "smallFrac shrinks nested arguments");
    assert_eq!(small.script().frac().level, 2, "smallFrac also shrinks display script fractions");
    let inline = St { display: false, fraction_size: FracSize::Script, ..st };
    assert_eq!(inline.script().frac().level, 2, "inline script fractions shrink their arguments");
}

#[test]
fn nested_fraction_size() {
    let db = FontDb::with_font_dirs(Vec::new());
    db.set_system_fallback(false);
    let font = db.face(wordcraft_fonts::DEFAULT_FAMILY, "Regular");
    let face = FaceRef::of(&font);
    let mut table = MathTable::default();
    table.script_percent = 73.0;
    table.script_script_percent = 60.0;
    let text = |s: &str| vec![MNode::Run(MRun::new(s))];
    let node = MNode::Frac { kind: FracKind::Bar, num: vec![MNode::Frac { kind: FracKind::Bar, num: text("a"), den: text("b") }], den: text("c") };
    for (display, fraction_size, inner_scale, outer_scale) in
        [(true, FracSize::Text, 1.0, 1.0), (true, FracSize::Display, 0.73, 1.0), (false, FracSize::Script, 0.6, 0.73)]
    {
        for size in [9.0, 14.0, 20.0] {
            let mut cx = Ctx {
                face,
                t: Some(Arc::new(table.clone())),
                upem: face.upem as f32,
                base: size,
                color: Rgb::BLACK,
                text_font: wordcraft_fonts::DEFAULT_FAMILY.into(),
                nodes: 0,
                path: Vec::new(),
                node_stack: Vec::new(),
            };
            let st = St { level: 0, display, fraction_size, frac_edges: FracEdges::Spaced, cramped: false, upright: false };
            let b = cx.arg(std::slice::from_ref(&node), st, 0);
            let sizes: Vec<f32> = b
                .items
                .iter()
                .filter_map(|item| match item {
                    MItem::Glyphs { size, .. } => Some(*size),
                    _ => None,
                })
                .collect();
            let expected = vec![size * inner_scale, size * inner_scale, size * outer_scale];
            assert_eq!(sizes.len(), expected.len());
            for (actual, expected) in sizes.iter().zip(expected) {
                assert!((actual - expected).abs() < 0.001, "display={display}: {sizes:?}");
            }
        }
    }
}

#[test]
fn mixed_size_script_base() {
    let db = FontDb::with_font_dirs(Vec::new());
    db.set_system_fallback(false);
    let font = db.face(wordcraft_fonts::DEFAULT_FAMILY, "Regular");
    let face = FaceRef::of(&font);
    for size in [9.0, 14.0, 20.0] {
        for ratio in [0.7, 1.0, 1.4] {
            for cramped in [false, true] {
                let mut cx = Ctx {
                    face,
                    t: None,
                    upem: face.upem as f32,
                    base: size,
                    color: Rgb::BLACK,
                    text_font: wordcraft_fonts::DEFAULT_FAMILY.into(),
                    nodes: 0,
                    path: Vec::new(),
                    node_stack: Vec::new(),
                };
                let st = St { level: 0, display: true, fraction_size: FracSize::Text, frac_edges: FracEdges::Spaced, cramped, upright: true };
                let run = |text: &str, point_size| {
                    MNode::Run(MRun { size: Some(point_size), sty: Some(wordcraft_doc::math::MSty::Plain), ..MRun::new(text) })
                };
                let base = vec![run("D", size * ratio)];
                let sup = vec![run("3", size)];
                let b = cx.arg(&base, st, 0);
                let p = cx.arg(&sup, st.script(), 0);
                let shift = cx.k(if cramped { C::SuperscriptShiftUpCramped } else { C::SuperscriptShiftUp }, st);
                let mut expected = shift.max(p.d + cx.k(C::SuperscriptBottomMin, st));
                if ratio != 1.0 {
                    expected = expected.max(b.a - cx.k(C::SuperscriptBaselineDropMax, st));
                }
                let result = cx.node(&MNode::Script { kind: ScriptKind::Sup, base, sub: Vec::new(), sup }, st, 0);
                let actual = result
                    .items
                    .iter()
                    .find_map(|item| match item {
                        MItem::Glyphs { text, glyphs, .. } if text == "3" => glyphs.first().map(|g| g.2),
                        _ => None,
                    })
                    .unwrap();
                assert!((actual - expected).abs() < 0.001, "size={size}, ratio={ratio}, cramped={cramped}: {actual} vs {expected}");
            }
        }
    }
}

#[test]
fn display_operators_are_big_with_limits_over_and_under() {
    let d = lay("∑_(i=1)^n▒i", true);
    let t = lay("∑_(i=1)^n▒i", false);
    assert!(d.ascent + d.descent > t.ascent + t.descent, "display sum taller");
    assert!(d.width < t.width + 4.0, "limits stacked in display");
}

#[test]
fn delimiter_size_and_alignment() {
    let db = FontDb::with_font_dirs(Vec::new());
    db.set_system_fallback(false);
    let font = db.face(wordcraft_fonts::DEFAULT_FAMILY, "Regular");
    let face = FaceRef::of(&font);
    let upem = face.upem as f32;
    let fitting = face.glyph_for('A');
    let oversized = face.glyph_for('B');
    let mut table = MathTable::default();
    table.vert.insert(face.glyph_for('('), Construction { variants: vec![(fitting, upem * 2.0), (oversized, upem * 3.0)], ..Default::default() });
    for size in [9.0, 14.0, 20.0] {
        let cx = Ctx {
            face,
            t: Some(Arc::new(table.clone())),
            upem,
            base: size,
            color: Rgb::BLACK,
            text_font: wordcraft_fonts::DEFAULT_FAMILY.into(),
            nodes: 0,
            path: Vec::new(),
            node_stack: Vec::new(),
        };
        let st = St { level: 0, display: false, fraction_size: FracSize::Script, frac_edges: FracEdges::Spaced, cramped: false, upright: false };
        for (a, d) in [(1.1 * size, 0.7 * size), (0.7 * size, 1.1 * size)] {
            for exact in [false, true] {
                let result = cx.delimiter('(', a, d, st, true, exact);
                let MItem::Glyphs { glyphs, .. } = &result.items[0] else { panic!("delimiter missing") };
                assert_eq!(glyphs[0].0, fitting, "shape alignment must not enlarge the content height");
                let middle = (result.a - result.d) / 2.0;
                let expected = if exact { (a - d) / 2.0 } else { cx.axis(st) };
                assert!((middle - expected).abs() < 0.001, "delimiter alignment");
            }
        }
    }
}

#[test]
fn delimiters_grow_around_tall_content() {
    let small = lay("(x)", true);
    let big = lay("((a)/(b))", true);
    let h = |m: &MathLayout| m.ascent + m.descent;
    assert!(h(&big) > h(&small) * 1.4, "{} vs {}", h(&big), h(&small));
}

#[test]
fn every_structure_lays_out() {
    use wordcraft_doc::math::*;
    let x = || vec![MNode::Run(MRun::new("x"))];
    let nodes = vec![
        MNode::Acc { chr: '\u{302}', e: x() },
        MNode::Acc { chr: '\u{305}', e: x() },
        MNode::Bar { top: false, e: x() },
        MNode::BorderBox { hide: [false; 4], strike: [true; 4], e: x() },
        MNode::Boxed { e: x() },
        MNode::GroupChr { chr: '⏟', top: false, e: x() },
        MNode::EqArr { rows: vec![vec![MNode::Run(MRun::new("x&=1"))], vec![MNode::Run(MRun::new("y&=22"))]] },
        MNode::Matrix { rows: vec![vec![x(), x()], vec![x(), Vec::new()]], col_jc: vec![] },
        MNode::Phant { show: false, zero_wid: false, zero_asc: false, zero_desc: true, e: x() },
        MNode::Lim { upper: false, e: vec![MNode::Run(MRun::plain("lim"))], lim: x() },
        MNode::Rad { deg: x(), deg_hide: false, e: x() },
        MNode::Script { kind: ScriptKind::Pre, base: x(), sub: x(), sup: x() },
        MNode::Frac { kind: FracKind::Skewed, num: x(), den: x() },
        MNode::Frac { kind: FracKind::Linear, num: x(), den: x() },
        MNode::Frac { kind: FracKind::NoBar, num: x(), den: x() },
        MNode::Delim { beg: None, end: Some('|'), sep: Some('|'), grow: true, shp_match: true, elems: vec![x(), x()] },
        MNode::Run(MRun { text: "text".into(), nor: true, ..Default::default() }),
        MNode::Run(MRun { text: "R".into(), scr: MScr::DoubleStruck, ..Default::default() }),
    ];
    for n in nodes {
        let m = layout_equation(&Math { nodes: vec![n.clone()], ..Default::default() }, "", &rc(), true, 400.0, &MathProps::default(), &mut 0);
        assert!(m.width > 0.0 && m.ascent + m.descent > 0.0, "{n:?} → {m:?}");
        for it in &m.items {
            if let MItem::Glyphs { glyphs, .. } = it {
                assert!(glyphs.iter().all(|g| g.1.is_finite() && g.2.is_finite()));
            }
        }
    }
}

/// #1273: an n-ary operator, delimiter, accent or group character the math font lacks is
/// drawn from a fallback font, not as a missing-glyph box.
#[test]
fn characters_the_math_font_lacks_fall_back() {
    let (face, _) = wordcraft_fonts::math::resolve_math("Cambria Math");
    let Some(c) = ['⨌', '⫿', '⟅', 'ア', '中', '가'].into_iter().find(|c| !face.covers(*c) && FontDb::global().fallback_for(*c, face.id()).is_some())
    else {
        return; // Every candidate is in the math font, or no fallback font is installed.
    };
    let x = || vec![MNode::Run(MRun::new("x"))];
    let nodes = [
        MNode::Nary { chr: c, lim_loc: None, grow: false, sub_hide: true, sup_hide: true, sub: vec![], sup: vec![], e: x() },
        MNode::Delim { beg: Some(c), end: Some(c), sep: None, grow: true, shp_match: false, elems: vec![x()] },
        MNode::Acc { chr: c, e: x() },
        MNode::GroupChr { chr: c, top: true, e: x() },
    ];
    for display in [false, true] {
        for n in &nodes {
            let m = layout_equation(&Math { nodes: vec![n.clone()], ..Default::default() }, "", &rc(), display, 400.0, &MathProps::default(), &mut 0);
            for it in &m.items {
                if let MItem::Glyphs { glyphs, .. } = it {
                    assert!(glyphs.iter().all(|g| g.0 != 0), "{c:?} drew a missing glyph in {n:?}");
                }
            }
        }
    }
}

#[test]
fn caret_slots_cover_every_position() {
    use wordcraft_doc::math_edit::{MathPos, move_right, units};
    let math = Math { nodes: wordcraft_doc::math::parse_linear("x=(-b±√(b^2-4ac))/2a+■(a&b@c&d)"), ..Default::default() };
    let m = layout_equation(&math, "", &rc(), true, 400.0, &MathProps::default(), &mut 0);
    // Walking the caret through the equation, every position has a slot.
    let mut p = MathPos::default();
    let mut seen = 0;
    loop {
        let s = m.slots.iter().find(|s| s.path == p.path && s.off == p.off);
        assert!(s.is_some(), "no slot for {p:?}");
        seen += 1;
        match move_right(&math.nodes, &p) {
            Some(n) => p = n,
            None => break,
        }
        assert!(seen < 500);
    }
    assert_eq!(p.off, units(&math.nodes));
    // Hit testing finds the denominator below the bar.
    let den = m.slots.iter().find(|s| s.path == vec![(1, 1)]).unwrap();
    let hit = m.hit(den.x + 1.0, den.y).unwrap();
    assert_eq!(hit.path, vec![(1, 1)]);
}

#[test]
fn an_arguments_extent_spans_its_caret_positions() {
    let math = Math { nodes: wordcraft_doc::math::parse_linear("y=(a+b)/c"), ..Default::default() };
    let m = layout_equation(&math, "", &rc(), true, 400.0, &MathProps::default(), &mut 0);
    let (nx0, nx1, nup, ndown) = m.arg_extent(&[(1, 0)]).unwrap();
    let (dx0, dx1, dup, ddown) = m.arg_extent(&[(1, 1)]).unwrap();
    // The numerator sits above the baseline and the denominator below it, both inside the equation.
    assert!(nx0 > 0.0 && nx1 > nx0 && nx1 <= m.width, "{nx0} {nx1} {}", m.width);
    assert!(dx1 > dx0 && dx1 <= m.width);
    assert!(-ndown > 0.0 && nup <= m.ascent + 0.01, "numerator {nup} {ndown}");
    assert!(dup < -ndown && ddown > 0.0 && ddown <= m.descent + 0.01, "denominator {dup} {ddown}");
    // The whole equation's argument is as wide as the equation.
    let (x0, x1, ..) = m.arg_extent(&[]).unwrap();
    assert!(x0.abs() < 0.01 && (x1 - m.width).abs() < 0.01, "{x0} {x1} {}", m.width);
    assert!(m.arg_extent(&[(9, 0)]).is_none());
}

#[test]
fn empty_slots_show_placeholders_on_screen_only() {
    let math = Math { nodes: wordcraft_doc::math::parse_linear("⬚/⬚"), ..Default::default() };
    let m = layout_equation(&math, "", &rc(), true, 400.0, &MathProps::default(), &mut 0);
    assert!(m.items.iter().all(|i| matches!(i, MItem::ScreenOnly(_) | MItem::Rect { .. })), "{:?}", m.items);
    assert!(m.slots.iter().any(|s| s.path == vec![(0, 0)] && s.off == 0));
    let empty = layout_equation(&Math::default(), "", &rc(), true, 400.0, &MathProps::default(), &mut 0);
    assert!(empty.width > 10.0 && empty.items.iter().all(|i| matches!(i, MItem::ScreenOnly(_))));
}

#[test]
fn numbered_equations_fill_the_line_with_the_number_at_the_right() {
    let math = Math { nodes: wordcraft_doc::math::parse_linear("E=mc^2#(1)"), ..Default::default() };
    let m = layout_equation(&math, "", &rc(), true, 400.0, &MathProps::default(), &mut 0);
    assert!((m.width - 400.0).abs() < 0.01, "{}", m.width);
    let xs: Vec<f32> = m.items.iter().filter_map(|i| if let MItem::Glyphs { glyphs, .. } = i { glyphs.first().map(|g| g.1) } else { None }).collect();
    let max = xs.iter().cloned().fold(0.0f32, f32::max);
    assert!(max > 350.0, "number at the right margin: {xs:?}");
    let min = xs.iter().cloned().fold(f32::MAX, f32::min);
    assert!(min > 120.0 && min < 220.0, "equation centred: {xs:?}");
    // Automatic numbers count.
    let auto = Math { nodes: wordcraft_doc::math::parse_linear("a=b#"), ..Default::default() };
    let mut n = 4;
    let _ = layout_equation(&auto, "", &rc(), true, 400.0, &MathProps::default(), &mut n);
    assert_eq!(n, 5);
    assert_eq!(auto_numbers(&auto, true), 1);
}

#[test]
fn math_alphabets() {
    assert_eq!(math_alnum('x', false, true, MScr::Roman), '𝑥');
    assert_eq!(math_alnum('h', false, true, MScr::Roman), 'ℎ');
    assert_eq!(math_alnum('R', false, false, MScr::DoubleStruck), 'ℝ');
    assert_eq!(math_alnum('α', false, true, MScr::Roman), '𝛼');
    assert_eq!(math_alnum('2', false, true, MScr::Roman), '2');
    assert_eq!(math_alnum('A', true, false, MScr::Roman), '𝐀');
}

#[test]
fn eqarr_rows_without_an_align_marker_are_centred() {
    use wordcraft_doc::math::*;
    // A row without `&` must not widen the alignment columns; it centres across the
    // array instead of right-aligning at the first column's edge.
    let rows: Vec<Vec<MNode>> = vec![vec![MNode::Run(MRun::new("mmm"))], vec![MNode::Run(MRun::new("x&yyyyyyyy"))]];
    let math = Math { nodes: vec![MNode::EqArr { rows }], ..Default::default() };
    let ls = lines(&lay_in(&math, "", true, 400.0, &MathProps::default()));
    assert_eq!(ls.len(), 2, "{ls:?}");
    let left: Vec<f32> = ls.iter().map(|(_, gs)| gs.iter().map(|(x, _)| *x).fold(f32::MAX, |a, b| if b < a { b } else { a })).collect();
    assert!(left.iter().all(|x| *x >= -0.5), "nothing hangs left of the array: {left:?}");
    // The short row starts right of the aligned row's first segment.
    assert!(ls[0].1.first().is_some_and(|(x, _)| *x > left[1] + 1.0), "short row centred, not flush left: {ls:?}");
}

#[test]
fn eqarr_row_without_an_align_marker_wider_than_the_columns_widens_the_array() {
    use wordcraft_doc::math::*;
    // The `&`-less row is wider than the aligned columns: it must not hang left of the
    // origin, the array is as wide as it, and the aligned rows centre under it.
    let rows: Vec<Vec<MNode>> = vec![vec![MNode::Run(MRun::new("bbbbbbbbbb"))], vec![MNode::Run(MRun::new("a&c"))]];
    let math = Math { nodes: vec![MNode::EqArr { rows }], ..Default::default() };
    let m = lay_in(&math, "", true, 400.0, &MathProps::default());
    let ls = lines(&m);
    assert_eq!(ls.len(), 2, "{ls:?}");
    let left: Vec<f32> = ls.iter().map(|(_, gs)| gs.iter().map(|(x, _)| *x).fold(f32::MAX, f32::min)).collect();
    assert!(left.iter().all(|x| *x >= -0.5), "nothing hangs left of the array: {left:?}");
    assert!(left[0].abs() < 0.5, "the wide row starts at the origin: {left:?}");
    let narrow = lay_in(
        &Math { nodes: vec![MNode::EqArr { rows: vec![vec![MNode::Run(MRun::new("a&c"))]] }], ..Default::default() },
        "",
        true,
        400.0,
        &MathProps::default(),
    );
    assert!(m.width > narrow.width + 1.0, "the array is as wide as its widest row: {} vs {}", m.width, narrow.width);
    assert!(left[1] > 1.0, "the aligned row is centred under the wide one: {left:?}");
}

#[test]
fn hostile_trees_never_panic() {
    use wordcraft_doc::math::*;
    // Deep nesting.
    let mut n = vec![MNode::Run(MRun::new("x"))];
    for _ in 0..500 {
        n = vec![MNode::Frac { kind: FracKind::Bar, num: n.clone(), den: Vec::new() }];
    }
    let _ = layout_equation(&Math { nodes: n, ..Default::default() }, "", &rc(), true, 400.0, &MathProps::default(), &mut 0);
    // Empty everything.
    let empty = vec![
        MNode::Nary { chr: '\0', lim_loc: None, grow: true, sub_hide: false, sup_hide: false, sub: vec![], sup: vec![], e: vec![] },
        MNode::Delim { beg: Some('\u{10FFFF}'), end: None, sep: None, grow: true, shp_match: false, elems: vec![] },
        MNode::Matrix { rows: vec![vec![]], col_jc: vec![] },
        MNode::EqArr { rows: vec![] },
        MNode::Run(MRun { text: String::new(), size: Some(f32::NAN), ..Default::default() }),
    ];
    let m = layout_equation(&Math { nodes: empty, ..Default::default() }, "", &rc(), true, 400.0, &MathProps::default(), &mut 0);
    assert!(m.width.is_finite() && m.ascent.is_finite());
    // Breaks everywhere, odd wrap settings, no room at all.
    let runs: Vec<MNode> = (0..2000).map(|i| MNode::Run(MRun { brk: (i % 3 == 0).then_some((i % 256) as u8), ..MRun::new("−+=") })).collect();
    for avail in [0.0, f32::NAN, 1e9] {
        for brk_bin in [BrkBin::Before, BrkBin::After, BrkBin::Repeat] {
            let mp =
                MathProps { brk_bin, brk_bin_sub: BrkBinSub::PlusMinus, wrap_indent: f32::INFINITY, wrap_right: avail == 0.0, ..Default::default() };
            let m = layout_equation(&Math { nodes: runs.clone(), ..Default::default() }, "", &rc(), true, avail, &mp, &mut 0);
            assert!(m.width.is_finite() && m.descent.is_finite());
        }
    }
}

/// Finding 23: each character's boundary in a run of text was found by walking the run's
/// glyphs from the first.
#[test]
fn a_long_text_run_lays_out_in_linear_time() {
    use crate::tests::linear::assert_linear_steps;
    use wordcraft_doc::math::*;
    let eq = |n: usize| Math { nodes: vec![MNode::Run(MRun { nor: true, ..MRun::new("a".repeat(n)) })], ..Default::default() };
    let lay = |m: &Math| drop(std::hint::black_box(lay_in(m, "", false, 1e6, &MathProps::default())));
    assert_linear_steps("math text run", 400, eq, lay);
}
