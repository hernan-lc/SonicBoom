//! Load-time ONNX rewrite that lets the Higgs codec decoder run on the WebGPU EP.
//!
//! On the AMD BC-250 (RADV) ORT's WebGPU `ConvTranspose` computes wrong values for 1-D
//! strides 3, 6, 7 and 12 (it fills only the first few outputs and leaves the rest at the
//! bias), which turns the decoder's output into near silence. Every other stride is exact.
//! The decoder has one such node (`block.4/conv_t1`, stride 3).
//!
//! The same operator can be written without `ConvTranspose`: zero-stuff the input to length
//! `(L - 1) * s + 1 + output_padding`, then run a stride-1 `Conv` with the weight transposed
//! (`[Cin, Cout, K]` -> `[Cout, Cin, K]`) and flipped along `K`, padded `K - 1 - p` on both
//! sides. This module performs that substitution on the serialized model, directly at the
//! protobuf wire level, so the pinned, SHA-verified decoder is the only file that has to exist
//! and no patched weights have to be hosted. Unknown fields are copied verbatim.
//!
//! The result is only used after a probe (see `native.rs`) proves it matches the CPU decoder.

/// 1-D `ConvTranspose` strides the WebGPU EP computes wrongly on RADV.
const BROKEN_STRIDES: [i64; 4] = [3, 6, 7, 12];

const FLOAT: u64 = 1;
const INT64: u64 = 7;
/// `Cast` target type FLOAT.
const CAST_TO_FLOAT: i64 = 1;

/// One decoded protobuf field. `raw` is the complete encoding (tag + payload), so a field can
/// be copied unchanged.
struct Field<'a> {
    number: u32,
    wire: u8,
    varint: u64,
    data: &'a [u8],
    raw: &'a [u8],
}

fn read_varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *buf.get(*pos)?;
        *pos += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn fields(buf: &[u8]) -> Option<Vec<Field<'_>>> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        let start = pos;
        let tag = read_varint(buf, &mut pos)?;
        let (number, wire) = ((tag >> 3) as u32, (tag & 7) as u8);
        let (mut varint, mut data) = (0, &buf[0..0]);
        match wire {
            0 => varint = read_varint(buf, &mut pos)?,
            1 => {
                data = buf.get(pos..pos + 8)?;
                pos += 8;
            }
            2 => {
                let len = usize::try_from(read_varint(buf, &mut pos)?).ok()?;
                data = buf.get(pos..pos.checked_add(len)?)?;
                pos += len;
            }
            5 => {
                data = buf.get(pos..pos + 4)?;
                pos += 4;
            }
            _ => return None,
        }
        out.push(Field {
            number,
            wire,
            varint,
            data,
            raw: &buf[start..pos],
        });
    }
    Some(out)
}

fn put_bytes(out: &mut Vec<u8>, number: u32, data: &[u8]) {
    write_varint(out, (u64::from(number) << 3) | 2);
    write_varint(out, data.len() as u64);
    out.extend_from_slice(data);
}

fn put_varint(out: &mut Vec<u8>, number: u32, value: u64) {
    write_varint(out, u64::from(number) << 3);
    write_varint(out, value);
}

/// Integers of a repeated field, accepting both the packed and the unpacked encoding.
fn ints(field: &Field<'_>) -> Option<Vec<i64>> {
    if field.wire == 0 {
        return Some(vec![field.varint as i64]);
    }
    let mut pos = 0;
    let mut out = Vec::new();
    while pos < field.data.len() {
        out.push(read_varint(field.data, &mut pos)? as i64);
    }
    Some(out)
}

struct Attribute {
    name: String,
    int: Option<i64>,
    ints: Vec<i64>,
    string: Option<String>,
}

fn attribute(buf: &[u8]) -> Option<Attribute> {
    let mut attr = Attribute {
        name: String::new(),
        int: None,
        ints: Vec::new(),
        string: None,
    };
    for f in fields(buf)? {
        match f.number {
            1 => attr.name = String::from_utf8(f.data.to_vec()).ok()?,
            3 => attr.int = Some(f.varint as i64),
            4 => attr.string = Some(String::from_utf8(f.data.to_vec()).ok()?),
            8 => attr.ints.extend(ints(&f)?),
            _ => {}
        }
    }
    Some(attr)
}

struct Node {
    inputs: Vec<String>,
    outputs: Vec<String>,
    name: String,
    op: String,
    attrs: Vec<Attribute>,
}

fn node(buf: &[u8]) -> Option<Node> {
    let mut n = Node {
        inputs: Vec::new(),
        outputs: Vec::new(),
        name: String::new(),
        op: String::new(),
        attrs: Vec::new(),
    };
    for f in fields(buf)? {
        match f.number {
            1 => n.inputs.push(String::from_utf8(f.data.to_vec()).ok()?),
            2 => n.outputs.push(String::from_utf8(f.data.to_vec()).ok()?),
            3 => n.name = String::from_utf8(f.data.to_vec()).ok()?,
            4 => n.op = String::from_utf8(f.data.to_vec()).ok()?,
            5 => n.attrs.push(attribute(f.data)?),
            _ => {}
        }
    }
    Some(n)
}

impl Node {
    fn attr(&self, name: &str) -> Option<&Attribute> {
        self.attrs.iter().find(|a| a.name == name)
    }
}

/// A float weight read from an initializer: dims and values.
struct Weight {
    dims: Vec<i64>,
    values: Vec<f32>,
}

fn weight(buf: &[u8]) -> Option<(String, Weight)> {
    let (mut name, mut dims, mut kind, mut raw) = (String::new(), Vec::new(), 0, None);
    for f in fields(buf)? {
        match f.number {
            1 => dims.extend(ints(&f)?),
            2 => kind = f.varint,
            8 => name = String::from_utf8(f.data.to_vec()).ok()?,
            9 => raw = Some(f.data),
            // external data (13/14) or typed float_data (4): not handled, leave the node alone
            13 | 14 | 4 => return None,
            _ => {}
        }
    }
    if kind != FLOAT {
        return None;
    }
    let raw = raw?;
    let count: i64 = dims.iter().product();
    if usize::try_from(count).ok()? * 4 != raw.len() {
        return None;
    }
    let values = raw
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect();
    Some((name, Weight { dims, values }))
}

fn tensor(name: &str, kind: u64, dims: &[i64], raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for d in dims {
        put_varint(&mut out, 1, *d as u64);
    }
    put_varint(&mut out, 2, kind);
    put_bytes(&mut out, 8, name.as_bytes());
    put_bytes(&mut out, 9, raw);
    out
}

fn int64_scalar(name: &str, value: i64) -> Vec<u8> {
    tensor(name, INT64, &[], &value.to_le_bytes())
}

fn attr_int(name: &str, value: i64) -> Vec<u8> {
    let mut out = Vec::new();
    put_bytes(&mut out, 1, name.as_bytes());
    put_varint(&mut out, 3, value as u64);
    put_varint(&mut out, 20, 2); // INT
    out
}

fn attr_ints(name: &str, values: &[i64]) -> Vec<u8> {
    let mut out = Vec::new();
    put_bytes(&mut out, 1, name.as_bytes());
    for v in values {
        put_varint(&mut out, 8, *v as u64);
    }
    put_varint(&mut out, 20, 7); // INTS
    out
}

fn make_node(
    op: &str,
    name: &str,
    inputs: &[&str],
    outputs: &[&str],
    attrs: &[Vec<u8>],
) -> Vec<u8> {
    let mut out = Vec::new();
    for i in inputs {
        put_bytes(&mut out, 1, i.as_bytes());
    }
    for o in outputs {
        put_bytes(&mut out, 2, o.as_bytes());
    }
    put_bytes(&mut out, 3, name.as_bytes());
    put_bytes(&mut out, 4, op.as_bytes());
    for a in attrs {
        put_bytes(&mut out, 5, a);
    }
    out
}

/// Geometry of a rewritable `ConvTranspose`.
struct Plan {
    stride: i64,
    kernel: i64,
    pad: i64,
    output_padding: i64,
}

/// `Some(plan)` when `n` is a 1-D `ConvTranspose` with a broken stride that this module knows
/// how to express as zero-stuffing + `Conv`.
fn plan(n: &Node, w: &Weight) -> Option<Plan> {
    let stride = *n.attr("strides")?.ints.first()?;
    if n.attr("strides")?.ints.len() != 1 || !BROKEN_STRIDES.contains(&stride) {
        return None;
    }
    if w.dims.len() != 3 || n.attr("group").and_then(|a| a.int).unwrap_or(1) != 1 {
        return None;
    }
    if n.attr("dilations")
        .is_some_and(|a| a.ints.iter().any(|&d| d != 1))
        || n.attr("output_shape").is_some()
        || n.attr("auto_pad")
            .and_then(|a| a.string.as_deref())
            .is_some_and(|s| s != "NOTSET")
    {
        return None;
    }
    let pad = match n.attr("pads") {
        None => 0,
        Some(a) if a.ints.len() == 2 && a.ints[0] == a.ints[1] => a.ints[0],
        Some(_) => return None,
    };
    let output_padding = match n.attr("output_padding") {
        None => 0,
        Some(a) if a.ints.len() == 1 => a.ints[0],
        Some(_) => return None,
    };
    let kernel = w.dims[2];
    if n.attr("kernel_shape").is_some_and(|k| k.ints != [kernel]) {
        return None;
    }
    (output_padding >= 0 && output_padding < stride && pad >= 0 && kernel - 1 - pad >= 0).then_some(
        Plan {
            stride,
            kernel,
            pad,
            output_padding,
        },
    )
}

/// `W'[o, c, k] = W[c, o, K - 1 - k]` as little-endian bytes (`[Cin, Cout, K]` -> `[Cout, Cin, K]`).
fn flipped_weight(w: &Weight) -> Vec<u8> {
    let (cin, cout, k) = (w.dims[0] as usize, w.dims[1] as usize, w.dims[2] as usize);
    let mut out = Vec::with_capacity(w.values.len() * 4);
    for o in 0..cout {
        for c in 0..cin {
            for kk in 0..k {
                out.extend_from_slice(&w.values[(c * cout + o) * k + (k - 1 - kk)].to_le_bytes());
            }
        }
    }
    out
}

/// The replacement subgraph (as serialized nodes) and the initializers it needs.
fn replacement(n: &Node, w: &Weight, plan: &Plan) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let base = if n.name.is_empty() {
        &n.outputs[0]
    } else {
        &n.name
    };
    let t = |suffix: &str| format!("{base}/sbfix/{suffix}");
    let (c0, c1, c2, cs, cx) = (t("zero"), t("one"), t("two"), t("stride"), t("extra"));
    let (axes, wn) = (t("axes"), t("weight"));
    let (shape, len, lm1, span, total) = (t("shape"), t("len"), t("lm1"), t("span"), t("total"));
    let (pos, idx, rem, hit, maskf, mask, gathered, stuffed) = (
        t("pos"),
        t("idx"),
        t("rem"),
        t("hit"),
        t("maskf"),
        t("mask"),
        t("gathered"),
        t("stuffed"),
    );
    let x = n.inputs[0].as_str();
    let mut conv_inputs = vec![stuffed.as_str(), wn.as_str()];
    if let Some(bias) = n.inputs.get(2).filter(|b| !b.is_empty()) {
        conv_inputs.push(bias);
    }
    let nodes = vec![
        make_node("Shape", &t("n_shape"), &[x], &[&shape], &[]),
        make_node(
            "Gather",
            &t("n_len"),
            &[&shape, &c2],
            &[&len],
            &[attr_int("axis", 0)],
        ),
        make_node("Sub", &t("n_lm1"), &[&len, &c1], &[&lm1], &[]),
        make_node("Mul", &t("n_span"), &[&lm1, &cs], &[&span], &[]),
        make_node("Add", &t("n_total"), &[&span, &cx], &[&total], &[]),
        make_node("Range", &t("n_pos"), &[&c0, &total, &c1], &[&pos], &[]),
        make_node("Div", &t("n_idx"), &[&pos, &cs], &[&idx], &[]),
        make_node("Mod", &t("n_rem"), &[&pos, &cs], &[&rem], &[]),
        make_node("Equal", &t("n_hit"), &[&rem, &c0], &[&hit], &[]),
        make_node(
            "Cast",
            &t("n_maskf"),
            &[&hit],
            &[&maskf],
            &[attr_int("to", CAST_TO_FLOAT)],
        ),
        make_node("Unsqueeze", &t("n_mask"), &[&maskf, &axes], &[&mask], &[]),
        make_node(
            "Gather",
            &t("n_gather"),
            &[x, &idx],
            &[&gathered],
            &[attr_int("axis", 2)],
        ),
        make_node("Mul", &t("n_stuff"), &[&gathered, &mask], &[&stuffed], &[]),
        make_node(
            "Conv",
            &t("n_conv"),
            &conv_inputs,
            &[&n.outputs[0]],
            &[
                attr_ints("kernel_shape", &[plan.kernel]),
                attr_ints("strides", &[1]),
                attr_ints(
                    "pads",
                    &[plan.kernel - 1 - plan.pad, plan.kernel - 1 - plan.pad],
                ),
                attr_ints("dilations", &[1]),
                attr_int("group", 1),
            ],
        ),
    ];
    let mut axes_raw = Vec::new();
    axes_raw.extend_from_slice(&0i64.to_le_bytes());
    axes_raw.extend_from_slice(&1i64.to_le_bytes());
    let inits = vec![
        int64_scalar(&c0, 0),
        int64_scalar(&c1, 1),
        int64_scalar(&c2, 2),
        int64_scalar(&cs, plan.stride),
        int64_scalar(&cx, 1 + plan.output_padding),
        tensor(&axes, INT64, &[2], &axes_raw),
        tensor(
            &wn,
            FLOAT,
            &[w.dims[1], w.dims[0], w.dims[2]],
            &flipped_weight(w),
        ),
    ];
    (nodes, inits)
}

/// Default-domain opset of the model, from `opset_import`.
fn default_opset(model: &[Field<'_>]) -> Option<i64> {
    model
        .iter()
        .filter(|f| f.number == 8)
        .filter_map(|f| {
            let parts = fields(f.data)?;
            // An unset domain (omitted by the serializer) is the default domain.
            let domain = parts
                .iter()
                .find(|p| p.number == 1)
                .map_or(&[][..], |p| p.data);
            let version = parts
                .iter()
                .find(|p| p.number == 2)
                .map(|p| p.varint as i64)?;
            (domain.is_empty() || domain == b"ai.onnx").then_some(version)
        })
        .next()
}

/// Rewrite every broken-stride `ConvTranspose` of `model` (a serialized `ModelProto`).
///
/// Returns the new model and the number of rewritten nodes, `Ok(None)` when no node needs the
/// rewrite, and `Err` when the model has a candidate this module cannot express safely.
pub fn rewrite_conv_transposes(model: &[u8]) -> Result<Option<(Vec<u8>, usize)>, String> {
    let model_fields = fields(model).ok_or("model is not valid protobuf")?;
    if default_opset(&model_fields).is_none_or(|v| v < 13) {
        return Err("default opset below 13".into());
    }
    let graph = model_fields
        .iter()
        .find(|f| f.number == 7 && f.wire == 2)
        .ok_or("model has no graph")?;
    let graph_fields = fields(graph.data).ok_or("graph is not valid protobuf")?;
    let weights: Vec<(String, Weight)> = graph_fields
        .iter()
        .filter(|f| f.number == 5)
        .filter_map(|f| weight(f.data))
        .collect();

    let mut new_graph = Vec::with_capacity(graph.data.len() + 4096);
    let mut extra_inits = Vec::new();
    let mut rewritten = 0;
    for f in &graph_fields {
        if f.number != 1 || f.wire != 2 {
            new_graph.extend_from_slice(f.raw);
            continue;
        }
        let n = node(f.data).ok_or("node is not valid protobuf")?;
        let is_candidate = n.op == "ConvTranspose"
            && n.attr("strides")
                .is_some_and(|a| a.ints.len() == 1 && BROKEN_STRIDES.contains(&a.ints[0]));
        if !is_candidate {
            new_graph.extend_from_slice(f.raw);
            continue;
        }
        let (_, w) = weights
            .iter()
            .find(|(name, _)| n.inputs.get(1) == Some(name))
            .ok_or_else(|| format!("weight of '{}' is not a plain float initializer", n.name))?;
        let p = plan(&n, w)
            .ok_or_else(|| format!("'{}' has attributes the rewrite does not cover", n.name))?;
        let (nodes, inits) = replacement(&n, w, &p);
        for node_bytes in nodes {
            put_bytes(&mut new_graph, 1, &node_bytes);
        }
        extra_inits.extend(inits);
        rewritten += 1;
    }
    if rewritten == 0 {
        return Ok(None);
    }
    for init in extra_inits {
        put_bytes(&mut new_graph, 5, &init);
    }
    let mut out = Vec::with_capacity(model.len() + new_graph.len() - graph.data.len() + 16);
    for f in &model_fields {
        if std::ptr::eq(f, graph) {
            put_bytes(&mut out, 7, &new_graph);
        } else {
            out.extend_from_slice(f.raw);
        }
    }
    Ok(Some((out, rewritten)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference ONNX `ConvTranspose` (1-D, group 1, dilation 1): scatter-add definition.
    #[allow(clippy::needless_range_loop)]
    fn conv_transpose(
        x: &[Vec<f32>],
        w: &[Vec<Vec<f32>>],
        bias: &[f32],
        p: &Plan,
    ) -> Vec<Vec<f32>> {
        let (cin, cout, l) = (x.len(), w[0].len(), x[0].len());
        let (s, k, pad, op) = (
            p.stride as usize,
            p.kernel as usize,
            p.pad as usize,
            p.output_padding as usize,
        );
        let lout = (l - 1) * s + k + op - 2 * pad;
        let mut y = vec![vec![0.0f32; lout]; cout];
        for o in 0..cout {
            for c in 0..cin {
                for t in 0..l {
                    for kk in 0..k {
                        let at = (t * s + kk) as isize - pad as isize;
                        if (0..lout as isize).contains(&at) {
                            y[o][at as usize] += x[c][t] * w[c][o][kk];
                        }
                    }
                }
            }
            for v in &mut y[o] {
                *v += bias[o];
            }
        }
        y
    }

    /// The rewrite's math: zero-stuff to `(L-1)s + 1 + op`, stride-1 cross-correlation with the
    /// flipped, transposed weight, padded `K-1-p` on both sides.
    #[allow(clippy::needless_range_loop)]
    fn stuffed_conv(x: &[Vec<f32>], w: &[Vec<Vec<f32>>], bias: &[f32], p: &Plan) -> Vec<Vec<f32>> {
        let (cin, cout, l) = (x.len(), w[0].len(), x[0].len());
        let (s, k, pad, op) = (
            p.stride as usize,
            p.kernel as usize,
            p.pad as usize,
            p.output_padding as usize,
        );
        let total = (l - 1) * s + 1 + op;
        let stuffed: Vec<Vec<f32>> = (0..cin)
            .map(|c| {
                (0..total)
                    .map(|i| if i % s == 0 { x[c][i / s] } else { 0.0 })
                    .collect()
            })
            .collect();
        let flat: Vec<f32> = w.iter().flat_map(|r| r.iter().flatten().copied()).collect();
        let flipped = flipped_weight(&Weight {
            dims: vec![cin as i64, cout as i64, k as i64],
            values: flat,
        });
        let wn = |o: usize, c: usize, kk: usize| {
            let at = ((o * cin + c) * k + kk) * 4;
            f32::from_le_bytes(flipped[at..at + 4].try_into().unwrap())
        };
        let left = k - 1 - pad;
        let lout = total + 2 * left - (k - 1);
        (0..cout)
            .map(|o| {
                (0..lout)
                    .map(|t| {
                        let mut acc = bias[o];
                        for c in 0..cin {
                            for kk in 0..k {
                                let at = (t + kk) as isize - left as isize;
                                if (0..total as isize).contains(&at) {
                                    acc += stuffed[c][at as usize] * wn(o, c, kk);
                                }
                            }
                        }
                        acc
                    })
                    .collect()
            })
            .collect()
    }

    fn pseudo(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        ((*seed >> 8) as f32 / (1u32 << 24) as f32) - 0.5
    }

    #[test]
    fn zero_stuffing_conv_equals_conv_transpose() {
        let mut seed = 7;
        // (stride, kernel, pad, output_padding): the decoder's block.4 node first.
        for (s, k, pad, op) in [
            (3, 6, 2, 1),
            (3, 6, 2, 0),
            (3, 4, 1, 0),
            (6, 12, 3, 1),
            (7, 14, 3, 1),
            (12, 24, 6, 0),
            (3, 6, 0, 2),
        ] {
            let p = Plan {
                stride: s,
                kernel: k,
                pad,
                output_padding: op,
            };
            for l in [1usize, 2, 5, 17] {
                let (cin, cout) = (3, 2);
                let x: Vec<Vec<f32>> = (0..cin)
                    .map(|_| (0..l).map(|_| pseudo(&mut seed)).collect())
                    .collect();
                let w: Vec<Vec<Vec<f32>>> = (0..cin)
                    .map(|_| {
                        (0..cout)
                            .map(|_| (0..k).map(|_| pseudo(&mut seed)).collect())
                            .collect()
                    })
                    .collect();
                let bias: Vec<f32> = (0..cout).map(|_| pseudo(&mut seed)).collect();
                let expect = conv_transpose(&x, &w, &bias, &p);
                let got = stuffed_conv(&x, &w, &bias, &p);
                assert_eq!(expect.len(), got.len());
                for (a, b) in expect.iter().zip(&got) {
                    assert_eq!(
                        a.len(),
                        b.len(),
                        "length for s={s} k={k} pad={pad} op={op} l={l}"
                    );
                    for (u, v) in a.iter().zip(b) {
                        assert!(
                            (u - v).abs() < 1e-5,
                            "s={s} k={k} pad={pad} op={op} l={l}: {u} vs {v}"
                        );
                    }
                }
            }
        }
    }

    fn float_tensor(name: &str, dims: &[i64], values: &[f32]) -> Vec<u8> {
        let raw: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        tensor(name, FLOAT, dims, &raw)
    }

    /// Minimal model: one `ConvTranspose` with the given stride, opset 18, plus an unknown field.
    fn tiny_model(stride: i64, with_external: bool) -> Vec<u8> {
        tiny_model_with_domain(stride, with_external, true)
    }

    fn tiny_model_with_domain(stride: i64, with_external: bool, write_domain: bool) -> Vec<u8> {
        let weights: Vec<f32> = (0..2 * 3 * 6).map(|i| i as f32).collect();
        let attrs = [
            attr_ints("kernel_shape", &[6]),
            attr_ints("strides", &[stride]),
            attr_ints("pads", &[2, 2]),
            attr_ints("output_padding", &[1]),
            attr_ints("dilations", &[1]),
            attr_int("group", 1),
        ];
        let mut graph = Vec::new();
        put_bytes(
            &mut graph,
            1,
            &make_node("ConvTranspose", "ct", &["x", "w", "b"], &["y"], &attrs),
        );
        put_bytes(&mut graph, 2, b"g");
        let mut w = float_tensor("w", &[2, 3, 6], &weights);
        if with_external {
            put_varint(&mut w, 14, 1);
        }
        put_bytes(&mut graph, 5, &w);
        put_bytes(&mut graph, 5, &float_tensor("b", &[3], &[0.5, 1.5, 2.5]));
        put_bytes(&mut graph, 99, b"unknown-field-survives");
        let mut opset = Vec::new();
        if write_domain {
            put_bytes(&mut opset, 1, b"");
        }
        put_varint(&mut opset, 2, 18);
        let mut model = Vec::new();
        put_varint(&mut model, 1, 9);
        put_bytes(&mut model, 7, &graph);
        put_bytes(&mut model, 8, &opset);
        model
    }

    fn graph_of(model: &[u8]) -> Vec<u8> {
        fields(model)
            .unwrap()
            .iter()
            .find(|f| f.number == 7)
            .unwrap()
            .data
            .to_vec()
    }

    #[test]
    fn rewrites_a_broken_stride_node_and_preserves_everything_else() {
        let model = tiny_model(3, false);
        let (patched, count) = rewrite_conv_transposes(&model).unwrap().unwrap();
        assert_eq!(count, 1);
        let graph = graph_of(&patched);
        let parsed = fields(&graph).unwrap();
        let ops: Vec<String> = parsed
            .iter()
            .filter(|f| f.number == 1)
            .map(|f| node(f.data).unwrap().op)
            .collect();
        assert!(!ops.contains(&"ConvTranspose".to_string()));
        assert_eq!(ops.iter().filter(|o| *o == "Conv").count(), 1);
        for needed in ["Shape", "Range", "Mod", "Equal", "Unsqueeze", "Gather"] {
            assert!(ops.contains(&needed.to_string()), "missing {needed}");
        }
        // The Conv writes the original output name and keeps the bias input.
        let conv = parsed
            .iter()
            .filter(|f| f.number == 1)
            .map(|f| node(f.data).unwrap())
            .find(|n| n.op == "Conv")
            .unwrap();
        assert_eq!(conv.outputs, ["y"]);
        assert_eq!(conv.inputs.len(), 3);
        assert_eq!(conv.inputs[2], "b");
        assert_eq!(conv.attr("pads").unwrap().ints, [3, 3]);
        // Unknown fields, opset and the original initializers are copied verbatim.
        assert!(
            parsed
                .iter()
                .any(|f| f.number == 99 && f.data == b"unknown-field-survives")
        );
        assert!(fields(&patched).unwrap().iter().any(|f| f.number == 8));
        // The new weight is the transposed, flipped original: W'[o,c,k] = W[c,o,5-k].
        let (_, w) = parsed
            .iter()
            .filter(|f| f.number == 5)
            .filter_map(|f| weight(f.data))
            .find(|(name, _)| name.ends_with("/sbfix/weight"))
            .unwrap();
        assert_eq!(w.dims, [3, 2, 6]);
        let at = |o: usize, c: usize, k: usize| w.values[(o * 2 + c) * 6 + k];
        assert_eq!(at(1, 0, 0), 11.0); // W[c=0][o=1][5]
        assert_eq!(at(2, 1, 5), 30.0); // W[c=1][o=2][0]
    }

    #[test]
    fn default_domain_may_be_omitted_from_the_opset_import() {
        // The real decoder is serialized this way; an explicit empty domain is also valid.
        for write_domain in [true, false] {
            let model = tiny_model_with_domain(3, false, write_domain);
            assert!(rewrite_conv_transposes(&model).unwrap().is_some());
        }
        let mut old_opset = tiny_model(3, false);
        // Same model but with opset 11 appended after the existing import: first wins.
        let mut opset = Vec::new();
        put_varint(&mut opset, 2, 11);
        put_bytes(&mut old_opset, 8, &opset);
        assert!(rewrite_conv_transposes(&old_opset).unwrap().is_some());
    }

    #[test]
    fn leaves_good_strides_and_unsupported_weights_alone() {
        assert!(
            rewrite_conv_transposes(&tiny_model(2, false))
                .unwrap()
                .is_none()
        );
        assert!(
            rewrite_conv_transposes(&tiny_model(8, false))
                .unwrap()
                .is_none()
        );
        assert!(rewrite_conv_transposes(&tiny_model(3, true)).is_err());
        assert!(rewrite_conv_transposes(b"not protobuf \xff\xff").is_err());
    }

    /// Needs the real decoder (`SONICBOOM_REVERB_DECODER`): exactly one node is rewritten.
    #[test]
    #[ignore = "needs models/reverb-tts/onnx/codec_decoder.onnx"]
    fn real_decoder_has_one_broken_node() {
        let path = std::env::var("SONICBOOM_REVERB_DECODER")
            .unwrap_or_else(|_| "models/reverb-tts/onnx/codec_decoder.onnx".into());
        let bytes = std::fs::read(path).unwrap();
        let (patched, count) = rewrite_conv_transposes(&bytes).unwrap().unwrap();
        assert_eq!(count, 1);
        assert!(patched.len() > bytes.len() / 2);
    }
}
