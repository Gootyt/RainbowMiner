//! Value helpers and number formats (like ConvertTo-Hash and the console of RainbowMiner)

use serde_json::Value;

pub fn get<'a>(v: &'a Value, k: &str) -> &'a Value {
    static NULL: Value = Value::Null;
    match v {
        Value::Object(m) => m.get(k).or_else(|| m.iter().find(|(x, _)| x.eq_ignore_ascii_case(k)).map(|(_, v)| v)).unwrap_or(&NULL),
        _ => &NULL,
    }
}

pub fn path<'a>(v: &'a Value, keys: &[&str]) -> &'a Value {
    keys.iter().fold(v, |cur, k| get(cur, k))
}

pub fn num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().parse().unwrap_or(0.0),
        Value::Bool(b) => *b as i64 as f64,
        Value::Array(a) => a.first().map(num).unwrap_or(0.0),
        _ => 0.0,
    }
}

pub fn opt(v: &Value) -> Option<f64> {
    match v {
        Value::Null => None,
        Value::String(s) if s.trim().is_empty() => None,
        _ => Some(num(v)),
    }
}

pub fn s(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(s).filter(|x| !x.is_empty()).collect::<Vec<_>>().join(", "),
        other => other.to_string(),
    }
}

pub fn list(v: &Value) -> Vec<&Value> {
    match v {
        Value::Array(a) => a.iter().collect(),
        Value::Object(m) => m.values().collect(),
        Value::Null => vec![],
        other => vec![other],
    }
}

pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(_) => true,
    }
}

/// ConvertTo-Hash: 12345678 -> "12.35 MH/s"
pub fn hash(h: f64) -> String {
    if h <= 0.0 || !h.is_finite() {
        return "-".into();
    }
    let units = ["H", "kH", "MH", "GH", "TH", "PH", "EH", "ZH"];
    let mut x = h;
    let mut i = 0;
    while x >= 1000.0 && i + 1 < units.len() {
        x /= 1000.0;
        i += 1;
    }
    format!("{:.2} {}/s", x, units[i])
}

/// a BTC amount with a sensible unit
pub fn btc(v: f64) -> String {
    let a = v.abs();
    if a == 0.0 {
        "0".into()
    } else if a < 0.001 {
        format!("{:.2} µBTC", v * 1e6)
    } else if a < 1.0 {
        format!("{:.4} mBTC", v * 1e3)
    } else {
        format!("{:.6} BTC", v)
    }
}

/// a currency amount: 2 decimals for fiat-like values, more for small ones
pub fn money(v: f64, cur: &str) -> String {
    let d = if v == 0.0 || v.abs() >= 100.0 {
        2
    } else if v.abs() >= 1.0 {
        3
    } else if v.abs() >= 0.01 {
        4
    } else {
        8
    };
    format!("{:.*} {}", d, v, cur)
}

/// seconds -> "1d 02:03:04"
pub fn duration(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "-".into();
    }
    let t = secs as u64;
    let (d, h, m, s) = (t / 86400, (t / 3600) % 24, (t / 60) % 60, t % 60);
    if d > 0 {
        format!("{}d {:02}:{:02}:{:02}", d, h, m, s)
    } else {
        format!("{:02}:{:02}:{:02}", h, m, s)
    }
}

/// a .NET timespan as RainbowMiner writes it ("1.02:03:04.5", "02:03:04") -> seconds
pub fn timespan(v: &Value) -> f64 {
    let t = s(v);
    let (days, rest) = match t.split_once('.') {
        Some((d, r)) if r.contains(':') && !d.contains(':') => (d.parse::<f64>().unwrap_or(0.0), r.to_string()),
        _ => (0.0, t.clone()),
    };
    let parts: Vec<f64> = rest.split(':').map(|x| x.parse::<f64>().unwrap_or(0.0)).collect();
    match parts.as_slice() {
        [h, m, s] => days * 86400.0 + h * 3600.0 + m * 60.0 + s,
        _ => 0.0,
    }
}

/// the uptime object of the API ({"Days":..,"Hours":..}) or a timespan string
pub fn uptime(v: &Value) -> f64 {
    if let Value::Object(_) = v {
        let total = num(get(v, "TotalSeconds"));
        if total > 0.0 {
            return total;
        }
        return num(get(v, "Days")) * 86400.0 + num(get(v, "Hours")) * 3600.0 + num(get(v, "Minutes")) * 60.0 + num(get(v, "Seconds"));
    }
    timespan(v)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    #[test]
    fn formats() {
        assert_eq!(super::hash(12345678.0), "12.35 MH/s");
        assert_eq!(super::hash(0.0), "-");
        assert_eq!(super::btc(0.00012), "120.00 µBTC");
        assert_eq!(super::btc(0.0123), "12.3000 mBTC");
        assert_eq!(super::duration(90061.0), "1d 01:01:01");
        assert_eq!(super::timespan(&json!("1.02:03:04.5")), 93784.5);
        assert_eq!(super::timespan(&json!("00:10:00")), 600.0);
        assert_eq!(super::uptime(&json!({"Days": 1, "Hours": 2, "Minutes": 3, "Seconds": 4})), 93784.0);
        assert_eq!(super::s(&json!(["a", "b"])), "a, b");
    }
}
