//! The slice of JavaScript value semantics the hooks were written against.
//!
//! The hooks used to be JavaScript, and other copies of them may still be
//! running beside this binary and reading the same files. So the port keeps
//! the language's own rules wherever a value is compared, printed or written:
//! truthiness, `Number(x)`, `String(x)`, `JSON.stringify` (key order included),
//! `new Date(x)`, and the `path` module's lexical rules. Each helper is small
//! and named for the expression it replaces.

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use std::cmp::Ordering;
use std::fmt;

/// A parsed JSON value that keeps object keys in the order they arrived, the
/// way `JSON.parse` does. A duplicate key keeps its first position and its
/// last value, also as `JSON.parse` does.
#[derive(Clone, Debug, PartialEq)]
pub enum J {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl<'de> Deserialize<'de> for J {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<J, D::Error> {
        d.deserialize_any(JVisitor)
    }
}

struct JVisitor;

impl<'de> Visitor<'de> for JVisitor {
    type Value = J;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_bool<E>(self, v: bool) -> Result<J, E> {
        Ok(J::Bool(v))
    }
    fn visit_i64<E>(self, v: i64) -> Result<J, E> {
        Ok(J::Num(v as f64))
    }
    fn visit_u64<E>(self, v: u64) -> Result<J, E> {
        Ok(J::Num(v as f64))
    }
    fn visit_f64<E>(self, v: f64) -> Result<J, E> {
        Ok(J::Num(v))
    }
    fn visit_str<E>(self, v: &str) -> Result<J, E> {
        Ok(J::Str(v.to_owned()))
    }
    fn visit_string<E>(self, v: String) -> Result<J, E> {
        Ok(J::Str(v))
    }
    fn visit_unit<E>(self) -> Result<J, E> {
        Ok(J::Null)
    }
    fn visit_none<E>(self) -> Result<J, E> {
        Ok(J::Null)
    }
    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<J, D::Error> {
        J::deserialize(d)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<J, A::Error> {
        let mut v = Vec::new();
        while let Some(x) = a.next_element::<J>()? {
            v.push(x);
        }
        Ok(J::Arr(v))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<J, A::Error> {
        let mut v: Vec<(String, J)> = Vec::new();
        while let Some((k, x)) = a.next_entry::<String, J>()? {
            set(&mut v, &k, x);
        }
        Ok(J::Obj(v))
    }
}

/// `JSON.parse`, or `None` where it would throw.
pub fn parse(s: &str) -> Option<J> {
    serde_json::from_str(s).ok()
}

/// Set a key in place, or append it: what `{ ...o, k: v }` does to key order.
pub fn set(o: &mut Vec<(String, J)>, k: &str, v: J) {
    if let Some(slot) = o.iter_mut().find(|(kk, _)| kk == k) {
        slot.1 = v;
    } else {
        o.push((k.to_string(), v));
    }
}

/// Build an object from literal pairs.
pub fn obj(pairs: Vec<(&str, J)>) -> J {
    J::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// `v.key`, where `None` stands for `undefined`.
pub fn get<'a>(v: Option<&'a J>, key: &str) -> Option<&'a J> {
    match v {
        Some(J::Obj(m)) => m.iter().find(|(k, _)| k == key).map(|(_, x)| x),
        _ => None,
    }
}

/// `undefined` or `null`.
pub fn nullish(v: Option<&J>) -> bool {
    matches!(v, None | Some(J::Null))
}

/// `a ?? b`
pub fn coalesce<'a>(a: Option<&'a J>, b: Option<&'a J>) -> Option<&'a J> {
    if nullish(a) {
        b
    } else {
        a
    }
}

/// `!!v`
pub fn truthy(v: Option<&J>) -> bool {
    match v {
        None | Some(J::Null) => false,
        Some(J::Bool(b)) => *b,
        Some(J::Num(n)) => *n != 0.0 && !n.is_nan(),
        Some(J::Str(s)) => !s.is_empty(),
        Some(J::Arr(_)) | Some(J::Obj(_)) => true,
    }
}

/// `typeof v === 'string' && v` as an `Option<&str>` (a non-empty string).
pub fn nonempty_str(v: Option<&J>) -> Option<&str> {
    match v {
        Some(J::Str(s)) if !s.is_empty() => Some(s.as_str()),
        _ => None,
    }
}

/// `String(v)`
pub fn to_string(v: Option<&J>) -> String {
    match v {
        None => "undefined".into(),
        Some(J::Null) => "null".into(),
        Some(J::Bool(b)) => b.to_string(),
        Some(J::Num(n)) => num_to_string(*n),
        Some(J::Str(s)) => s.clone(),
        Some(J::Arr(a)) => a
            .iter()
            .map(|e| if nullish(Some(e)) { String::new() } else { to_string(Some(e)) })
            .collect::<Vec<_>>()
            .join(","),
        Some(J::Obj(_)) => "[object Object]".into(),
    }
}

/// `Number(v)`
pub fn to_number(v: Option<&J>) -> f64 {
    match v {
        None => f64::NAN,
        Some(J::Null) => 0.0,
        Some(J::Bool(b)) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Some(J::Num(n)) => *n,
        Some(J::Str(s)) => str_to_number(s),
        Some(J::Arr(_)) | Some(J::Obj(_)) => str_to_number(&to_string(v)),
    }
}

/// `Number(v || 0)`
pub fn num_or_zero(v: Option<&J>) -> f64 {
    if truthy(v) {
        to_number(v)
    } else {
        0.0
    }
}

/// JavaScript's StringToNumber.
pub fn str_to_number(s: &str) -> f64 {
    let t = trim(s);
    if t.is_empty() {
        return 0.0;
    }
    match t {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    let lower = t.get(..2).map(|p| p.to_ascii_lowercase());
    let radix = match lower.as_deref() {
        Some("0x") => Some(16),
        Some("0o") => Some(8),
        Some("0b") => Some(2),
        _ => None,
    };
    if let Some(r) = radix {
        let digits = &t[2..];
        if digits.is_empty() || !digits.chars().all(|c| c.is_digit(r)) {
            return f64::NAN;
        }
        return digits.chars().fold(0.0, |acc, c| acc * r as f64 + c.to_digit(r).unwrap() as f64);
    }
    // StrDecimalLiteral: sign? (digits [. digits?] | . digits) exponent?
    let b = t.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut digits = i - int_start;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let f = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        digits += i - f;
    }
    if digits == 0 {
        return f64::NAN;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let e = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == e {
            return f64::NAN;
        }
    }
    if i != b.len() {
        return f64::NAN;
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// JavaScript's Number::toString(10).
pub fn num_to_string(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n == 0.0 {
        return "0".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    let neg = n < 0.0;
    // Rust's `{:e}` is the shortest round-trip digit string, which is what
    // JavaScript prints; only the layout rules differ.
    let e = format!("{:e}", n.abs());
    let (mant, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let nn = exp + 1;
    let body = if k <= nn && nn <= 21 {
        format!("{}{}", digits, "0".repeat((nn - k) as usize))
    } else if 0 < nn && nn <= 21 {
        format!("{}.{}", &digits[..nn as usize], &digits[nn as usize..])
    } else if -6 < nn && nn <= 0 {
        format!("0.{}{}", "0".repeat((-nn) as usize), digits)
    } else {
        let sign = if nn > 0 { '+' } else { '-' };
        let (d0, rest) = digits.split_at(1);
        if rest.is_empty() {
            format!("{}e{}{}", d0, sign, (nn - 1).abs())
        } else {
            format!("{}.{}e{}{}", d0, rest, sign, (nn - 1).abs())
        }
    };
    if neg {
        format!("-{body}")
    } else {
        body
    }
}

// ── JSON.stringify ──────────────────────────────────────────────────────────

/// `JSON.stringify(v)`
pub fn stringify(v: &J) -> String {
    let mut out = String::new();
    write_json(&mut out, v, None, 0);
    out
}

/// `JSON.stringify(v, null, 2)`
pub fn stringify_pretty(v: &J) -> String {
    let mut out = String::new();
    write_json(&mut out, v, Some(2), 0);
    out
}

/// A property key that JavaScript orders before every other key: a canonical
/// array index.
fn index_key(k: &str) -> Option<u64> {
    if k.is_empty() || k.len() > 10 || (k.len() > 1 && k.starts_with('0')) {
        return None;
    }
    if !k.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u64 = k.parse().ok()?;
    (n < u32::MAX as u64).then_some(n)
}

/// Keys in JavaScript's own-property order: indices ascending, then the rest
/// in insertion order.
pub fn ordered(m: &[(String, J)]) -> Vec<&(String, J)> {
    let mut idx: Vec<(u64, &(String, J))> =
        m.iter().filter_map(|e| index_key(&e.0).map(|n| (n, e))).collect();
    idx.sort_by_key(|(n, _)| *n);
    let mut out: Vec<&(String, J)> = idx.into_iter().map(|(_, e)| e).collect();
    out.extend(m.iter().filter(|e| index_key(&e.0).is_none()));
    out
}

fn write_json(out: &mut String, v: &J, indent: Option<usize>, depth: usize) {
    match v {
        J::Null => out.push_str("null"),
        J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        J::Num(n) => {
            if n.is_finite() {
                out.push_str(&num_to_string(*n))
            } else {
                out.push_str("null")
            }
        }
        J::Str(s) => quote_into(out, s),
        J::Arr(a) => {
            if a.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, e) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, indent, depth + 1);
                write_json(out, e, indent, depth + 1);
            }
            newline(out, indent, depth);
            out.push(']');
        }
        J::Obj(m) => {
            if m.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (k, e)) in ordered(m).into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, indent, depth + 1);
                quote_into(out, k);
                out.push(':');
                if indent.is_some() {
                    out.push(' ');
                }
                write_json(out, e, indent, depth + 1);
            }
            newline(out, indent, depth);
            out.push('}');
        }
    }
}

fn newline(out: &mut String, indent: Option<usize>, depth: usize) {
    if let Some(n) = indent {
        out.push('\n');
        out.push_str(&" ".repeat(n * depth));
    }
}

fn quote_into(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

// ── strings ─────────────────────────────────────────────────────────────────

/// A character JavaScript's `\s` and `trim()` treat as white space.
pub fn is_ws(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

/// A character a JavaScript `.` does not match, and where `^`/`$` stop under `m`.
pub fn is_line_term(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `s.trim()`
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_ws)
}

/// An ASCII `\w` character.
pub fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The length JavaScript reports: UTF-16 code units.
pub fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `s.slice(0, n)`, counted in UTF-16 code units. A cut through a surrogate
/// pair drops the half JavaScript would keep as a lone surrogate, which a
/// Rust string cannot hold.
pub fn utf16_prefix(s: &str, n: usize) -> String {
    let mut used = 0;
    let mut out = String::new();
    for c in s.chars() {
        let w = c.len_utf16();
        if used + w > n {
            break;
        }
        used += w;
        out.push(c);
    }
    out
}

/// The default `Array.prototype.sort` order for strings: UTF-16 code units.
pub fn utf16_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// Case-insensitive ASCII `starts_with`, as a regex under the `i` flag reads
/// an ASCII literal.
pub fn starts_with_ci(hay: &str, lit: &str) -> bool {
    hay.len() >= lit.len()
        && hay.as_bytes()[..lit.len()].eq_ignore_ascii_case(lit.as_bytes())
}

/// Case-insensitive ASCII substring test.
pub fn contains_ci(hay: &str, lit: &str) -> bool {
    hay.char_indices().any(|(i, _)| starts_with_ci(&hay[i..], lit))
}

// ── time ────────────────────────────────────────────────────────────────────

/// `Date.now()`
pub fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `new Date(ms).toISOString()` for a finite time value.
pub fn iso_string(ms: f64) -> String {
    let ms = ms as i64;
    let days = ms.div_euclid(86_400_000);
    let rem = ms.rem_euclid(86_400_000);
    let (y, m, d) = civil_from_days(days);
    let year = if (0..=9999).contains(&y) {
        format!("{y:04}")
    } else if y < 0 {
        format!("-{:06}", -y)
    } else {
        format!("+{y:06}")
    };
    format!(
        "{}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year,
        m,
        d,
        rem / 3_600_000,
        (rem / 60_000) % 60,
        (rem / 1000) % 60,
        rem % 1000
    )
}

fn time_clip(t: f64) -> f64 {
    if !t.is_finite() || t.abs() > 8.64e15 {
        f64::NAN
    } else {
        t.trunc() + 0.0
    }
}

/// `new Date(v).getTime()` for one argument.
pub fn date_value(v: Option<&J>) -> f64 {
    match v {
        None => f64::NAN,
        Some(J::Str(s)) => parse_date(s),
        Some(J::Arr(_)) => parse_date(&to_string(v)),
        Some(J::Obj(_)) => f64::NAN,
        other => time_clip(to_number(other)),
    }
}

/// `Date.parse(s)` for the ISO forms (the forms every writer of these files
/// uses). A date alone is UTC; a date and time with no offset is local time.
pub fn parse_date(s: &str) -> f64 {
    parse_iso(s).unwrap_or(f64::NAN)
}

fn parse_iso(s: &str) -> Option<f64> {
    let b = s.as_bytes();
    let mut i = 0;
    let num = |i: &mut usize, n: usize| -> Option<i64> {
        let part = b.get(*i..*i + n)?;
        if !part.iter().all(|c| c.is_ascii_digit()) {
            return None;
        }
        *i += n;
        std::str::from_utf8(part).ok()?.parse().ok()
    };
    let year = match b.first()? {
        b'+' | b'-' => {
            let neg = b[0] == b'-';
            i = 1;
            let y = num(&mut i, 6)?;
            if neg && y == 0 {
                return None;
            }
            if neg {
                -y
            } else {
                y
            }
        }
        _ => num(&mut i, 4)?,
    };
    let (mut month, mut day) = (1, 1);
    if b.get(i) == Some(&b'-') {
        i += 1;
        month = num(&mut i, 2)?;
        if b.get(i) == Some(&b'-') {
            i += 1;
            day = num(&mut i, 2)?;
        }
    }
    let (mut h, mut mi, mut sec, mut ms) = (0, 0, 0, 0.0);
    let mut has_time = false;
    let mut offset_min: Option<i64> = None;
    if b.get(i) == Some(&b'T') {
        has_time = true;
        i += 1;
        h = num(&mut i, 2)?;
        if b.get(i) != Some(&b':') {
            return None;
        }
        i += 1;
        mi = num(&mut i, 2)?;
        if b.get(i) == Some(&b':') {
            i += 1;
            sec = num(&mut i, 2)?;
            if b.get(i) == Some(&b'.') {
                i += 1;
                let start = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                if i == start {
                    return None;
                }
                let frac = std::str::from_utf8(&b[start..i]).ok()?;
                let three: String = frac.chars().chain("000".chars()).take(3).collect();
                ms = three.parse::<f64>().ok()?;
            }
        }
        match b.get(i) {
            Some(b'Z') => {
                offset_min = Some(0);
                i += 1;
            }
            Some(&c) if c == b'+' || c == b'-' => {
                i += 1;
                let oh = num(&mut i, 2)?;
                if b.get(i) != Some(&b':') {
                    return None;
                }
                i += 1;
                let om = num(&mut i, 2)?;
                let o = oh * 60 + om;
                offset_min = Some(if c == b'-' { -o } else { o });
            }
            _ => {}
        }
    }
    if i != b.len() {
        return None;
    }
    let mdays = [31, if is_leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if !(1..=12).contains(&month) || day < 1 || day > mdays[(month - 1) as usize] {
        return None;
    }
    if h > 24 || mi > 59 || sec > 59 || (h == 24 && (mi > 0 || sec > 0 || ms > 0.0)) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    let wall = (days * 86_400 + h * 3600 + mi * 60 + sec) as f64 * 1000.0 + ms;
    let t = match (has_time, offset_min) {
        (_, Some(o)) => wall - (o * 60_000) as f64,
        (false, None) => wall,
        (true, None) => local_to_utc(year, month, day, h, mi, sec)? * 1000.0 + ms,
    };
    Some(time_clip(t))
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn local_to_utc(y: i64, mo: i64, d: i64, h: i64, mi: i64, s: i64) -> Option<f64> {
    // SAFETY: an all-zero tm is a valid value, and mktime only reads and
    // normalises the struct it is given.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = (y - 1900) as libc::c_int;
    tm.tm_mon = (mo - 1) as libc::c_int;
    tm.tm_mday = d as libc::c_int;
    tm.tm_hour = h as libc::c_int;
    tm.tm_min = mi as libc::c_int;
    tm.tm_sec = s as libc::c_int;
    tm.tm_isdst = -1;
    let t = unsafe { libc::mktime(&mut tm) };
    (t != -1).then_some(t as f64)
}

// ── node:path (posix) ───────────────────────────────────────────────────────

fn normalize_string(p: &str, allow_above_root: bool) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if out.last().is_some_and(|l| *l != "..") {
                    out.pop();
                } else if allow_above_root {
                    out.push("..");
                }
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

/// `path.normalize(p)`
pub fn normalize(p: &str) -> String {
    if p.is_empty() {
        return ".".into();
    }
    let abs = p.starts_with('/');
    let trailing = p.ends_with('/');
    let mut s = normalize_string(p, !abs);
    if s.is_empty() {
        if abs {
            return "/".into();
        }
        return if trailing { "./".into() } else { ".".into() };
    }
    if trailing {
        s.push('/');
    }
    if abs {
        format!("/{s}")
    } else {
        s
    }
}

/// `path.join(...parts)`
pub fn join(parts: &[&str]) -> String {
    let joined: Vec<&str> = parts.iter().copied().filter(|p| !p.is_empty()).collect();
    if joined.is_empty() {
        return ".".into();
    }
    normalize(&joined.join("/"))
}

/// `process.cwd()`
pub fn process_cwd() -> Option<String> {
    std::env::current_dir().ok().map(|p| p.to_string_lossy().into_owned())
}

/// `path.resolve(...parts)`
pub fn resolve(parts: &[&str]) -> String {
    let mut resolved = String::new();
    let mut abs = false;
    let cwd = process_cwd().unwrap_or_else(|| "/".into());
    let all: Vec<&str> = std::iter::once(cwd.as_str()).chain(parts.iter().copied()).collect();
    for p in all.iter().rev() {
        if abs {
            break;
        }
        if p.is_empty() {
            continue;
        }
        resolved = if resolved.is_empty() { p.to_string() } else { format!("{p}/{resolved}") };
        abs = p.starts_with('/');
    }
    let s = normalize_string(&resolved, !abs);
    if abs {
        format!("/{s}")
    } else if s.is_empty() {
        ".".into()
    } else {
        s
    }
}

/// `path.isAbsolute(p)`
pub fn is_absolute(p: &str) -> bool {
    p.starts_with('/')
}

/// `path.relative(from, to)`
pub fn relative(from: &str, to: &str) -> String {
    let f = resolve(&[from]);
    let t = resolve(&[to]);
    if f == t {
        return String::new();
    }
    let fc: Vec<&str> = f.split('/').filter(|s| !s.is_empty()).collect();
    let tc: Vec<&str> = t.split('/').filter(|s| !s.is_empty()).collect();
    let common = fc.iter().zip(tc.iter()).take_while(|(a, b)| a == b).count();
    let mut out: Vec<&str> = vec![".."; fc.len() - common];
    out.extend(&tc[common..]);
    out.join("/")
}

/// `path.dirname(p)`
pub fn dirname(p: &str) -> String {
    if p.is_empty() {
        return ".".into();
    }
    let b = p.as_bytes();
    let has_root = b[0] == b'/';
    let mut end: Option<usize> = None;
    let mut matched_slash = true;
    for i in (1..b.len()).rev() {
        if b[i] == b'/' {
            if !matched_slash {
                end = Some(i);
                break;
            }
        } else {
            matched_slash = false;
        }
    }
    match end {
        None => if has_root { "/".into() } else { ".".into() },
        Some(1) if has_root => "//".into(),
        Some(e) => p[..e].to_string(),
    }
}

/// `os.homedir()`: `$HOME` when set, else the password database.
pub fn home_dir() -> Option<String> {
    if let Ok(h) = std::env::var("HOME") {
        if !h.is_empty() {
            return Some(h);
        }
    }
    // SAFETY: getpwuid returns a pointer into static storage or null; it is
    // read once, on this thread, before any other call could overwrite it.
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() || (*pw).pw_dir.is_null() {
            return None;
        }
        Some(std::ffi::CStr::from_ptr((*pw).pw_dir).to_string_lossy().into_owned())
    }
}

/// `fs.readFileSync(p, 'utf8')`
pub fn read_utf8(p: &str) -> std::io::Result<String> {
    std::fs::read(p).map(|b| String::from_utf8_lossy(&b).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_the_way_javascript_prints_them() {
        for (n, s) in [
            (0.0, "0"),
            (-0.0, "0"),
            (5.0, "5"),
            (1.5, "1.5"),
            (0.1, "0.1"),
            (1e21, "1e+21"),
            (1e20, "100000000000000000000"),
            (1.5e-7, "1.5e-7"),
            (0.000001, "0.000001"),
            (-42.0, "-42"),
            (1759670000123.0, "1759670000123"),
        ] {
            assert_eq!(num_to_string(n), s, "{n}");
        }
        assert_eq!(num_to_string(f64::NAN), "NaN");
    }

    #[test]
    fn string_to_number_follows_the_language() {
        assert_eq!(str_to_number(" 3 "), 3.0);
        assert_eq!(str_to_number(""), 0.0);
        assert_eq!(str_to_number("0x1f"), 31.0);
        assert!(str_to_number("inf").is_nan());
        assert!(str_to_number("3px").is_nan());
        assert_eq!(str_to_number("1e3"), 1000.0);
        assert_eq!(str_to_number(".5"), 0.5);
    }

    #[test]
    fn key_order_survives_a_round_trip() {
        let v = parse(r#"{"b":1,"a":{"z":true,"y":null},"c":"x"}"#).unwrap();
        assert_eq!(stringify(&v), r#"{"b":1,"a":{"z":true,"y":null},"c":"x"}"#);
        // Index keys lead, as they do in any JavaScript object.
        let v = parse(r#"{"b":1,"2":2,"1":3}"#).unwrap();
        assert_eq!(stringify(&v), r#"{"1":3,"2":2,"b":1}"#);
        // A duplicate keeps its first place and its last value.
        let v = parse(r#"{"a":1,"b":2,"a":3}"#).unwrap();
        assert_eq!(stringify(&v), r#"{"a":3,"b":2}"#);
    }

    #[test]
    fn strings_escape_as_json_stringify_does() {
        assert_eq!(stringify(&J::Str("a\"b\\c\n\u{7}\u{1f}é".into())), "\"a\\\"b\\\\c\\n\\u0007\\u001fé\"");
    }

    #[test]
    fn pretty_matches_two_space_indent() {
        let v = obj(vec![("a", J::Num(1.0)), ("b", J::Null)]);
        assert_eq!(stringify_pretty(&v), "{\n  \"a\": 1,\n  \"b\": null\n}");
    }

    #[test]
    fn iso_round_trip() {
        let t = parse_date("2026-10-05T12:34:56.789Z");
        assert_eq!(iso_string(t), "2026-10-05T12:34:56.789Z");
        assert_eq!(parse_date("1970-01-01"), 0.0);
        assert_eq!(parse_date("1970-01-01T00:00:00+01:00"), -3_600_000.0);
        assert!(parse_date("not a date").is_nan());
        assert_eq!(date_value(Some(&J::Num(1000.0))), 1000.0);
        assert_eq!(date_value(Some(&J::Null)), 0.0);
        assert!(date_value(None).is_nan());
    }

    #[test]
    fn paths_follow_node() {
        assert_eq!(join(&["/a/b", "../c", ".designless"]), "/a/c/.designless");
        assert_eq!(dirname("/tmp/designless-501/other.sock"), "/tmp/designless-501");
        assert_eq!(dirname("ipc.sock"), ".");
        assert_eq!(relative("/a/b", "/a/b/c/d"), "c/d");
        assert_eq!(relative("/a/b/c", "/a/b"), "..");
        assert_eq!(resolve(&["/x", "y", "../z"]), "/x/z");
        assert_eq!(normalize("/a//b/./c/"), "/a/b/c/");
    }

    #[test]
    fn truthiness_and_string_conversion() {
        assert!(!truthy(Some(&J::Str(String::new()))));
        assert!(truthy(Some(&J::Arr(vec![]))));
        assert!(!truthy(Some(&J::Num(f64::NAN))));
        assert_eq!(to_string(Some(&J::Arr(vec![J::Num(1.0), J::Null, J::Str("x".into())]))), "1,,x");
        assert_eq!(to_string(None), "undefined");
        assert_eq!(num_or_zero(Some(&J::Str("3".into()))), 3.0);
    }
}
