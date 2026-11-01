//! Schema-driven encoding: the reverse of [`crate::decode`]. A value in the form
//! `decode_type` produces is written back to subrecord bytes, so a record decoded,
//! edited and encoded keeps every byte it did not change.
//!
//! Where decoding loses information the encoder cannot restore it, and says so
//! instead of guessing: a float that was NaN or infinite decodes to JSON null, bytes
//! after a zero-terminated string's NUL are dropped, and an enum whose number has
//! two names writes the first. `fnvtool esm encode-check` counts such cases over
//! whole plugins.

use crate::decode::{decode_sub, DecodeCtx, SubStatus};
use crate::raw::Subrecord;
use crate::schema::{Prim, Schema, TypeDef};
use fnv_util::cp1252;
use serde_json::Value;

fn from_hex(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) {
        return Err(format!("hex of odd length: {s:?}"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| format!("not hex: {s:?}")))
        .collect()
}

fn text(s: &str) -> Result<Vec<u8>, String> {
    cp1252::encode(s).ok_or_else(|| format!("{s:?} has a character Windows-1252 cannot hold"))
}

fn int_of(v: &Value) -> Result<i128, String> {
    v.as_i64().map(i128::from).or_else(|| v.as_u64().map(i128::from)).ok_or_else(|| format!("expected an integer, got {v}"))
}

/// Write `n` as the integer type `p`, refusing values that do not fit.
fn write_int(p: Prim, n: i128, out: &mut Vec<u8>) -> Result<(), String> {
    let range = |lo: i128, hi: i128| if (lo..=hi).contains(&n) { Ok(()) } else { Err(format!("{n} does not fit in {p:?}")) };
    match p {
        Prim::U8 => range(0, u8::MAX as i128).map(|_| out.push(n as u8)),
        Prim::U16 => range(0, u16::MAX as i128).map(|_| out.extend((n as u16).to_le_bytes())),
        Prim::U32 => range(0, u32::MAX as i128).map(|_| out.extend((n as u32).to_le_bytes())),
        Prim::U64 => range(0, u64::MAX as i128).map(|_| out.extend((n as u64).to_le_bytes())),
        Prim::I8 => range(i8::MIN as i128, i8::MAX as i128).map(|_| out.push(n as i8 as u8)),
        Prim::I16 => range(i16::MIN as i128, i16::MAX as i128).map(|_| out.extend((n as i16).to_le_bytes())),
        Prim::I32 => range(i32::MIN as i128, i32::MAX as i128).map(|_| out.extend((n as i32).to_le_bytes())),
        Prim::I64 => range(i64::MIN as i128, i64::MAX as i128).map(|_| out.extend((n as i64).to_le_bytes())),
        _ => Err("non-integer base".into()),
    }
}

/// Integers as `read_int` reads them: a u64 base is read into an i64, so its top
/// half arrives negative.
fn write_base(p: Prim, n: i128, out: &mut Vec<u8>) -> Result<(), String> {
    match (p, n) {
        (Prim::U64, n) if n < 0 => write_int(p, n + (1i128 << 64), out),
        _ => write_int(p, n, out),
    }
}

fn write_prim(p: Prim, v: &Value, out: &mut Vec<u8>, map: &mut dyn FnMut(u32) -> u32) -> Result<(), String> {
    let s = || v.as_str().ok_or_else(|| format!("expected a string, got {v}"));
    match p {
        Prim::U8 | Prim::U16 | Prim::U32 | Prim::U64 | Prim::I8 | Prim::I16 | Prim::I32 | Prim::I64 => write_int(p, int_of(v)?, out),
        Prim::F32 => {
            let f = v.as_f64().ok_or_else(|| format!("expected a number, got {v} (NaN and infinities decode to null)"))?;
            out.extend((f as f32).to_le_bytes());
            Ok(())
        }
        Prim::F64 => {
            let f = v.as_f64().ok_or_else(|| format!("expected a number, got {v} (NaN and infinities decode to null)"))?;
            out.extend(f.to_le_bytes());
            Ok(())
        }
        Prim::FormId => {
            let t = s()?;
            let h = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")).unwrap_or(t);
            let id = u32::from_str_radix(h, 16).map_err(|_| format!("not a FormID: {t:?}"))?;
            out.extend(map(id).to_le_bytes());
            Ok(())
        }
        Prim::ZString => {
            out.extend(text(s()?)?);
            out.push(0);
            Ok(())
        }
        Prim::ZStringArray => {
            let items = v.as_array().ok_or_else(|| format!("expected a list of strings, got {v}"))?;
            // Each string NUL-terminated, then the NUL that closes the list [observed:
            // every CREA KFFZ/NIFZ in the ten masters, schema.rs]. An empty list is no bytes.
            for it in items {
                out.extend(text(it.as_str().ok_or_else(|| format!("expected a string, got {it}"))?)?);
                out.push(0);
            }
            if !items.is_empty() {
                out.push(0);
            }
            Ok(())
        }
        Prim::String => {
            out.extend(text(s()?)?);
            Ok(())
        }
        Prim::LString => {
            let b = text(s()?)?;
            out.extend(u32::try_from(b.len()).map_err(|_| "string too long".to_string())?.to_le_bytes());
            out.extend(b);
            Ok(())
        }
        Prim::Bytes => {
            out.extend(from_hex(s()?)?);
            Ok(())
        }
    }
}

/// Append the bytes of `v`, a value of type `t` in the form `decode_type` gives.
pub fn encode_type(t: &TypeDef, v: &Value, schema: &Schema, out: &mut Vec<u8>) -> Result<(), String> {
    encode_type_mapped(t, v, schema, out, &mut |id| id)
}

/// [`encode_type`], writing every FormID field through `map` (a record copied into
/// another plugin has its FormIDs' master indexes rewritten, `crate::edit`).
pub fn encode_type_mapped(t: &TypeDef, v: &Value, schema: &Schema, out: &mut Vec<u8>, map: &mut dyn FnMut(u32) -> u32) -> Result<(), String> {
    match schema.resolve(t) {
        TypeDef::Prim(p) => write_prim(*p, v, out, map),
        TypeDef::Named(_) => Err("unresolved type".into()),
        TypeDef::Union(variants) => {
            // Decoding chose by the bytes left: the first variant whose size matched,
            // else the first without a size. Encoding tries the sized variants in order
            // and keeps the first that comes out at its own size, else the fallback.
            for (size, vt) in variants.iter().filter(|(n, _)| n.is_some()) {
                let mut tmp = Vec::new();
                if encode_type_mapped(vt, v, schema, &mut tmp, map).is_ok() && Some(tmp.len()) == *size {
                    out.extend(tmp);
                    return Ok(());
                }
            }
            let (_, vt) = variants.iter().find(|(n, _)| n.is_none()).ok_or("no union variant fits the value")?;
            encode_type_mapped(vt, v, schema, out, map)
        }
        TypeDef::Struct(fields) => {
            let obj = v.as_object().ok_or_else(|| format!("expected an object, got {v}"))?;
            let mut ended = None;
            for f in fields {
                match obj.get(&f.name) {
                    Some(fv) => {
                        if let Some(gap) = ended {
                            return Err(format!("{}: present after the absent optional field {gap}", f.name));
                        }
                        encode_type_mapped(&f.ty, fv, schema, out, map).map_err(|e| format!("{}: {e}", f.name))?;
                    }
                    None if f.optional => ended = ended.or(Some(f.name.as_str())),
                    None => return Err(format!("{}: missing", f.name)),
                }
            }
            Ok(())
        }
        TypeDef::Array { elem, count, prefix } => {
            let items = v.as_array().ok_or_else(|| format!("expected a list, got {v}"))?;
            if let Some(p) = prefix {
                write_int(*p, items.len() as i128, out)?;
            }
            if let Some(n) = count {
                if items.len() != *n {
                    return Err(format!("{} elements where the layout has {n}", items.len()));
                }
            }
            for (i, it) in items.iter().enumerate() {
                encode_type_mapped(elem, it, schema, out, map).map_err(|e| format!("[{i}]: {e}"))?;
            }
            Ok(())
        }
        TypeDef::Enum { base, values } => {
            let n = match v {
                Value::String(name) => values.iter().find(|(_, n)| n == name).map(|(k, _)| *k as i128).ok_or_else(|| format!("no value named {name:?}"))?,
                _ => int_of(v)?,
            };
            write_base(*base, n, out)
        }
        TypeDef::Flags { base, bits } => {
            // The decoded form keeps the whole number; the names are for reading.
            if let Some(n) = v.get("value") {
                return write_base(*base, int_of(n)?, out);
            }
            let names = v.get("set").and_then(Value::as_array).ok_or_else(|| format!("expected flags, got {v}"))?;
            let mut n = 0u64;
            for name in names {
                let name = name.as_str().ok_or("flag names are strings")?;
                let (b, _) = bits.iter().find(|(_, bn)| bn == name).ok_or_else(|| format!("no flag named {name:?}"))?;
                n |= 1 << b;
            }
            write_base(*base, n as i128, out)
        }
    }
}

/// The bytes of a subrecord `sig` of a `rec` record holding `v`.
pub fn encode_sub(rec: &[u8; 4], sig: &[u8; 4], v: &Value, schema: &Schema) -> Result<Vec<u8>, String> {
    let def = schema.lookup(rec, sig).ok_or_else(|| format!("no definition for {} in {}", fnv_util::tag_str(sig), fnv_util::tag_str(rec)))?;
    let mut out = Vec::new();
    encode_type(&def.ty, v, schema, &mut out)?;
    Ok(out)
}

/// How a subrecord came back through decode and encode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoundTrip {
    /// The encoded bytes equal the subrecord's (for a Trailing decode: the decoded part).
    Same,
    /// Not decoded (Unknown or Error): nothing to encode; the editor keeps such bytes raw.
    NotDecoded,
    /// Encoding refused the value.
    Refused(String),
    /// Encoded, but to other bytes: (first differing offset, encoded length).
    Differs(usize, usize),
}

/// Decode `s` with the schema, encode the value, and compare with the source bytes.
pub fn round_trip(rec: &[u8; 4], s: &Subrecord, schema: &Schema) -> RoundTrip {
    let mut ctx = DecodeCtx { schema, on_formid: None };
    let d = decode_sub(rec, s, &mut ctx);
    let Some(v) = d.value.filter(|_| matches!(d.status, SubStatus::Ok | SubStatus::Trailing)) else { return RoundTrip::NotDecoded };
    let got = match encode_sub(rec, &s.sig, &v, schema) {
        Ok(b) => b,
        Err(e) => return RoundTrip::Refused(e),
    };
    let want: &[u8] = match d.status {
        // A trailing decode stopped early; the decoded part is the prefix it read, which
        // is as long as the encoding when the round trip works.
        SubStatus::Trailing if got.len() < s.data.len() => &s.data[..got.len()],
        _ => &s.data,
    };
    if got == want {
        RoundTrip::Same
    } else {
        let at = got.iter().zip(want).position(|(a, b)| a != b).unwrap_or(got.len().min(want.len()));
        RoundTrip::Differs(at, got.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::decode_type;
    use fnv_util::Cursor;

    const S: &str = r#"{"format":"fnv.schema/1",
      "types":{
        "Alt":{"struct":[{"name":"shape","type":"lstring"},{"name":"set","type":"formid"},{"name":"index","type":"i32"}]},
        "Kind":{"enum":"u8","values":{"0":"Topic","1":"Conversation"}},
        "Fl":{"flags":"u16","bits":{"0":"Goodbye","2":"Say Once"}},
        "Opt":{"struct":[{"name":"a","type":"u8"},{"name":"b","type":"f32","optional":true},{"name":"c","type":"i16","optional":true}]},
        "Un":{"union":[{"size":2,"type":"u16"},{"type":{"struct":[{"name":"x","type":"u32"},{"name":"y","type":"u32"}]}}]}
      },
      "common":{"EDID":{"type":"zstring"},"MODS":{"type":{"array":"Alt","count":"u32"}},"KFFZ":{"type":"zstring_array"}},
      "records":{"TEST":{"subrecords":{
        "DATA":{"type":{"struct":[{"name":"kind","type":"Kind"},{"name":"flags","type":"Fl"},{"name":"f","type":"f32"},{"name":"d","type":"f64"},{"name":"i","type":"i8"},{"name":"u","type":"u64"}]}},
        "OPTS":{"type":"Opt"},"UNIO":{"type":"Un"},"RAW_":{"type":"bytes"},"STR_":{"type":"string"},
        "GRID":{"type":{"array":"i16","count":3}},"FILL":{"type":{"array":"formid"}}}}}}"#;

    fn schema() -> Schema {
        let mut s = Schema::default();
        s.merge_str(S, "t").unwrap();
        s.validate().unwrap();
        s
    }

    /// Decode `bytes` as `sig`, encode, and expect the same bytes.
    fn same(s: &Schema, sig: &[u8; 4], bytes: &[u8]) {
        let sub = Subrecord { sig: *sig, data: bytes.to_vec(), extended: false };
        assert_eq!(round_trip(b"TEST", &sub, s), RoundTrip::Same, "{}", fnv_util::tag_str(sig));
    }

    #[test]
    fn every_shape_comes_back_byte_for_byte() {
        let s = schema();
        same(&s, b"EDID", b"VMS38RedLucy\0");
        let mut mods = vec![2, 0, 0, 0];
        for (name, fid, idx) in [("GO:1", 0x53363u32, 1i32), ("arms", 0x6bcf3, -1)] {
            mods.extend((name.len() as u32).to_le_bytes());
            mods.extend(name.as_bytes());
            mods.extend(fid.to_le_bytes());
            mods.extend(idx.to_le_bytes());
        }
        same(&s, b"MODS", &mods);
        same(&s, b"KFFZ", b"a.kf\0b.kf\0\0");
        same(&s, b"KFFZ", b"");
        let mut data = vec![1u8, 0x05, 0x00];
        data.extend(0.1f32.to_le_bytes());
        data.extend((-0.0f64).to_le_bytes());
        data.push(0x80);
        data.extend(u64::MAX.to_le_bytes());
        same(&s, b"DATA", &data);
        // An enum number without a name, and flag bits without names.
        data[0] = 7;
        data[1] = 0xFF;
        same(&s, b"DATA", &data);
        same(&s, b"OPTS", &[9]);
        same(&s, b"OPTS", &[9, 0, 0, 128, 63]);
        same(&s, b"OPTS", &[9, 0, 0, 128, 63, 0xFE, 0xFF]);
        same(&s, b"UNIO", &[1, 2]);
        same(&s, b"UNIO", &[1, 0, 0, 0, 2, 0, 0, 0]);
        same(&s, b"RAW_", &[0xDE, 0xAD]);
        same(&s, b"STR_", &[]);
        same(&s, b"STR_", &[b'c', b'a', b'f', 0xE9]);
        same(&s, b"GRID", &[1, 0, 2, 0, 0xFF, 0xFF]);
        same(&s, b"FILL", &[1, 0, 0, 0, 0x14, 0, 0, 0]);
    }

    #[test]
    fn edits_encode_and_decode_back() {
        let s = schema();
        let ty = s.lookup(b"TEST", b"DATA").unwrap().ty.clone();
        let v = serde_json::json!({"kind": "Conversation", "flags": {"set": ["Goodbye", "Say Once"]}, "f": 2.5, "d": 1e300, "i": -3, "u": 7});
        let mut b = Vec::new();
        encode_type(&ty, &v, &s, &mut b).unwrap();
        let back = decode_type(&ty, &mut Cursor::new(&b), &mut DecodeCtx { schema: &s, on_formid: None }).unwrap();
        assert_eq!(back["kind"], "Conversation");
        assert_eq!(back["flags"]["value"], 5);
        assert_eq!(back["f"], 2.5);
        assert_eq!(back["i"], -3);
    }

    #[test]
    fn refuses_what_does_not_fit() {
        let s = schema();
        let ty = s.lookup(b"TEST", b"GRID").unwrap().ty.clone();
        let mut b = Vec::new();
        assert!(encode_type(&ty, &serde_json::json!([1, 2]), &s, &mut b).unwrap_err().contains("elements"));
        assert!(encode_type(&ty, &serde_json::json!([1, 2, 40000]), &s, &mut b).unwrap_err().contains("does not fit"));
        assert!(encode_sub(b"TEST", b"EDID", &serde_json::json!("\u{4e2d}"), &s).unwrap_err().contains("Windows-1252"));
        assert!(encode_sub(b"TEST", b"OPTS", &serde_json::json!({"a": 1, "c": 2}), &s).unwrap_err().contains("absent optional"));
        assert!(encode_sub(b"TEST", b"DATA", &serde_json::json!({"kind": "Nope"}), &s).is_err());
        // NaN decodes to null, which cannot be written back.
        let mut nan = vec![0u8, 0, 0];
        nan.extend(f32::NAN.to_le_bytes());
        nan.extend([0; 17]);
        let sub = Subrecord { sig: *b"DATA", data: nan, extended: false };
        assert!(matches!(round_trip(b"TEST", &sub, &s), RoundTrip::Refused(_)));
    }
}
