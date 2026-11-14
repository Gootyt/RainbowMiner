//! Get-BestMinerDeviceCombos / Get-ExactDeviceCovers: the most profitable set of miners that
//! covers every device exactly once.

use std::collections::HashMap;

/// one candidate: its device names and the sort values
pub struct ComboItem {
    pub devices: Vec<String>,
    pub profit_is_null: bool,
    pub sort_value: f64,
}

fn device_key(d: &[String]) -> String {
    let mut v: Vec<String> = d.to_vec();
    v.sort();
    v.join("|")
}

fn exact_covers(device_count: usize, group_ix: &[Option<Vec<usize>>], by_device: &[Vec<usize>], covered: &mut Vec<bool>, num: usize, chosen: &mut Vec<usize>, covers: &mut Vec<Vec<usize>>) {
    if num == device_count {
        covers.push(chosen.clone());
        return;
    }
    let Some(dev) = (0..device_count).find(|i| !covered[*i]) else { return };
    for &g in &by_device[dev] {
        let Some(ix) = &group_ix[g] else { continue };
        if ix.iter().any(|i| covered[*i]) {
            continue;
        }
        for i in ix {
            covered[*i] = true;
        }
        chosen.push(g);
        exact_covers(device_count, group_ix, by_device, covered, num + ix.len(), chosen, covers);
        chosen.pop();
        for i in ix {
            covered[*i] = false;
        }
    }
}

/// returns the indices of the chosen items
pub fn best_combo(items: &[ComboItem]) -> Vec<usize> {
    if items.is_empty() {
        return vec![];
    }
    let mut device_names: Vec<String> = items.iter().flat_map(|m| m.devices.iter().cloned()).collect();
    device_names.sort();
    device_names.dedup();
    // distinct device sets in first appearance order
    let mut groups: Vec<Vec<String>> = Vec::new();
    for m in items {
        if !groups.iter().any(|g| device_key(g) == device_key(&m.devices)) {
            groups.push(m.devices.clone());
        }
    }
    let dev_ix: HashMap<&str, usize> = device_names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    let mut group_ix: Vec<Option<Vec<usize>>> = Vec::new();
    let mut by_device: Vec<Vec<usize>> = vec![Vec::new(); device_names.len()];
    for (gi, g) in groups.iter().enumerate() {
        let mut ix = Vec::new();
        let mut dup = false;
        for d in g {
            let j = dev_ix[d.as_str()];
            if ix.contains(&j) {
                dup = true;
                break;
            }
            ix.push(j);
        }
        if dup || ix.is_empty() {
            group_ix.push(None);
            continue;
        }
        for j in &ix {
            by_device[*j].push(gi);
        }
        group_ix.push(Some(ix));
    }
    let mut covers = Vec::new();
    exact_covers(device_names.len(), &group_ix, &by_device, &mut vec![false; device_names.len()], 0, &mut Vec::new(), &mut covers);
    // emission order of the subset walk: group count ascending, then the descending index lists
    covers.sort_by(|a, b| {
        a.len().cmp(&b.len()).then_with(|| {
            let mut x = a.clone();
            let mut y = b.clone();
            x.sort_by(|p, q| q.cmp(p));
            y.sort_by(|p, q| q.cmp(p));
            x.cmp(&y)
        })
    });
    let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, m) in items.iter().enumerate() {
        by_key.entry(device_key(&m.devices)).or_default().push(i);
    }
    let mut best: Option<(usize, f64, Vec<usize>)> = None;
    for c in covers {
        let mut sel = Vec::new();
        for g in c {
            if let Some(v) = by_key.get(&device_key(&groups[g])) {
                sel.extend(v.iter().copied());
            }
        }
        let nulls = sel.iter().filter(|i| items[**i].profit_is_null).count();
        let sum: f64 = sel.iter().map(|i| items[*i].sort_value).sum();
        let better = match &best {
            None => true,
            Some((bn, bs, _)) => nulls > *bn || (nulls == *bn && sum > *bs),
        };
        if better {
            best = Some((nulls, sum, sel));
        }
    }
    best.map(|b| b.2).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn it(d: &[&str], v: f64) -> ComboItem {
        ComboItem {
            devices: d.iter().map(|s| s.to_string()).collect(),
            profit_is_null: false,
            sort_value: v,
        }
    }

    #[test]
    fn picks_best_cover() {
        // two single GPUs (1+1) vs. the combo (3)
        let items = vec![it(&["GPU#00"], 1.0), it(&["GPU#01"], 1.0), it(&["GPU#00", "GPU#01"], 3.0), it(&["CPU#00"], 0.5)];
        let mut r = best_combo(&items);
        r.sort();
        assert_eq!(r, vec![2, 3]);
        let items = vec![it(&["GPU#00"], 2.0), it(&["GPU#01"], 2.0), it(&["GPU#00", "GPU#01"], 3.0)];
        let mut r = best_combo(&items);
        r.sort();
        assert_eq!(r, vec![0, 1]);
    }

    #[test]
    fn benchmarks_first() {
        let mut a = it(&["GPU#00"], 0.0);
        a.profit_is_null = true;
        let items = vec![a, it(&["GPU#01"], 1.0), it(&["GPU#00", "GPU#01"], 5.0)];
        let mut r = best_combo(&items);
        r.sort();
        assert_eq!(r, vec![0, 1]);
    }
}
