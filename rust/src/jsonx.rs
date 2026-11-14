//! serde_json helpers that mimic PowerShell's forgiving object access:
//! case-insensitive property names, member enumeration over arrays and lenient number casts.

use serde_json::{Map, Value};

pub static NULL: Value = Value::Null;

/// Case-insensitive property lookup (exact match first)
pub fn get<'a>(v: &'a Value, key: &str) -> &'a Value {
    match v {
        Value::Object(m) => {
            if let Some(x) = m.get(key) {
                return x;
            }
            for (k, x) in m.iter() {
                if k.eq_ignore_ascii_case(key) {
                    return x;
                }
            }
            &NULL
        }
        _ => &NULL,
    }
}

/// Nested property lookup, e.g. path(v, &["result","mining","hashrate"])
pub fn path<'a>(v: &'a Value, keys: &[&str]) -> &'a Value {
    let mut cur = v;
    for k in keys {
        cur = get(cur, k);
        if cur.is_null() {
            return &NULL;
        }
    }
    cur
}

/// Index into an array (out of range -> null)
pub fn idx(v: &Value, i: usize) -> &Value {
    match v {
        Value::Array(a) => a.get(i).unwrap_or(&NULL),
        _ => {
            if i == 0 {
                v
            } else {
                &NULL
            }
        }
    }
}

/// [double]$x
pub fn num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                0.0
            } else {
                t.parse::<f64>()
                    .or_else(|_| t.replace(',', ".").parse::<f64>())
                    .unwrap_or_else(|_| {
                        // "123abc" -> leading number
                        let lead: String = t
                            .chars()
                            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == 'e' || *c == 'E' || *c == '+')
                            .collect();
                        lead.parse::<f64>().unwrap_or(0.0)
                    })
            }
        }
        Value::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Value::Array(a) if a.len() == 1 => num(&a[0]),
        _ => 0.0,
    }
}

/// Like num(), but None for null / missing / not a number
pub fn num_opt(v: &Value) -> Option<f64> {
    match v {
        Value::Null => None,
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                t.parse::<f64>().ok().or_else(|| t.replace(',', ".").parse::<f64>().ok())
            }
        }
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::Array(a) if a.len() == 1 => num_opt(&a[0]),
        _ => None,
    }
}

pub fn int(v: &Value) -> i64 {
    num(v) as i64
}

/// "$x" (string interpolation)
pub fn s(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => if *b { "True".into() } else { "False".into() },
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(u) = n.as_u64() {
                u.to_string()
            } else {
                let f = n.as_f64().unwrap_or(0.0);
                if f.fract() == 0.0 && f.abs() < 1e15 {
                    format!("{}", f as i64)
                } else {
                    format!("{}", f)
                }
            }
        }
        Value::Array(a) => a.iter().map(s).collect::<Vec<_>>().join(" "),
        Value::Object(_) => v.to_string(),
    }
}

/// PowerShell truthiness ("if ($x)")
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => match a.len() {
            0 => false,
            1 => truthy(&a[0]),
            _ => true,
        },
        Value::Object(_) => true,
    }
}

/// Member enumeration: $arr.field -> flattened list of the field values
pub fn pluck<'a>(v: &'a Value, key: &str) -> Vec<&'a Value> {
    let mut out = Vec::new();
    match v {
        Value::Array(a) => {
            for x in a {
                let f = get(x, key);
                match f {
                    Value::Null => {}
                    Value::Array(inner) => out.extend(inner.iter()),
                    _ => out.push(f),
                }
            }
        }
        Value::Object(_) => {
            let f = get(v, key);
            match f {
                Value::Null => {}
                Value::Array(inner) => out.extend(inner.iter()),
                _ => out.push(f),
            }
        }
        _ => {}
    }
    out
}

/// ($arr.field | Measure-Object -Sum).Sum
pub fn sum_field(v: &Value, key: &str) -> f64 {
    pluck(v, key).into_iter().map(num).sum()
}

/// (... | Measure-Object -Sum).Sum over a value that may be a scalar or an array
pub fn sum(v: &Value) -> f64 {
    match v {
        Value::Array(a) => a.iter().map(num).sum(),
        _ => num(v),
    }
}

/// Values of an object's properties ($obj.PSObject.Properties.Value)
pub fn values(v: &Value) -> Vec<&Value> {
    match v {
        Value::Object(m) => m.values().collect(),
        _ => Vec::new(),
    }
}

/// Property names of an object
pub fn keys(v: &Value) -> Vec<String> {
    match v {
        Value::Object(m) => m.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

/// Iterate over a value as list: arrays yield elements, scalars yield themselves, null yields nothing
pub fn as_list(v: &Value) -> Vec<&Value> {
    match v {
        Value::Null => Vec::new(),
        Value::Array(a) => a.iter().collect(),
        _ => vec![v],
    }
}

/// Number of properties of an object (or elements of an array)
pub fn count(v: &Value) -> usize {
    match v {
        Value::Object(m) => m.len(),
        Value::Array(a) => a.len(),
        Value::Null => 0,
        _ => 1,
    }
}

/// String list from a value that may be an array or a "a,b;c" string
pub fn str_list(v: &Value) -> Vec<String> {
    match v {
        Value::Null => Vec::new(),
        Value::Array(a) => a
            .iter()
            .map(s)
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect(),
        Value::String(st) => crate::util::split_config_array(st),
        other => {
            let x = s(other);
            if x.is_empty() {
                Vec::new()
            } else {
                vec![x]
            }
        }
    }
}

/// Set a property in an object (insert or overwrite)
pub fn set(v: &mut Value, key: &str, val: Value) {
    if !v.is_object() {
        *v = Value::Object(Map::new());
    }
    if let Value::Object(m) = v {
        // overwrite case-insensitively matching key, keep its spelling
        let existing = m.keys().find(|k| k.eq_ignore_ascii_case(key)).cloned();
        match existing {
            Some(k) => {
                m.insert(k, val);
            }
            None => {
                m.insert(key.to_string(), val);
            }
        }
    }
}

/// Get a mutable property (case-insensitive)
pub fn get_mut<'a>(v: &'a mut Value, key: &str) -> Option<&'a mut Value> {
    match v {
        Value::Object(m) => {
            let k = m.keys().find(|k| k.eq_ignore_ascii_case(key)).cloned()?;
            m.get_mut(&k)
        }
        _ => None,
    }
}

/// Parse JSON leniently: strip BOM, trailing garbage like PowerShell's ConvertFrom-Json would not accept
pub fn parse(text: &str) -> Option<Value> {
    let t = text.trim_start_matches('\u{feff}').trim();
    if t.is_empty() {
        return None;
    }
    serde_json::from_str(t).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn access() {
        let v = json!({"Devices":[{"speed":1},{"speed":"2.5"}],"a":{"B":3}});
        assert_eq!(sum_field(get(&v, "devices"), "speed"), 3.5);
        assert_eq!(num(path(&v, &["a", "b"])), 3.0);
        assert!(get(&v, "missing").is_null());
        assert_eq!(s(&json!(1.0)), "1");
    }
}
