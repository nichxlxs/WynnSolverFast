//! Applies Layer2::apply_radiance to a synthetic stat map and prints the
//! result, so it can be diffed against the JS _apply_radiance_scale_inplace.
//! Usage: radiance_check <score fixture> <boost>
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&a[1]).expect("fixture");
    let mut v: serde_json::Value = serde_json::from_str(&text).expect("json");
    let boost: f64 = a[2].parse().expect("boost");
    v["layer2"]["radiance_boost"] = serde_json::json!(boost);
    let l2 = sp_kernel::scoring::Layer2::parse(&v).expect("layer2");

    // Deterministic probe values covering positive, negative and zero.
    let mut sm = sp_kernel::scoring::Obj::new();
    for (i, id) in l2.radiance_affected.iter().enumerate() {
        let val = match i % 4 {
            0 => 37.0, 1 => -23.0, 2 => 0.0, _ => 101.5,
        };
        sm.insert(id.clone(), serde_json::json!(val));
    }
    l2.apply_radiance(&mut sm);
    let mut out: Vec<(String, f64)> = sm.iter()
        .map(|(k, v)| (k.clone(), v.as_f64().unwrap())).collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    for (k, val) in out { println!("{} {}", k, val); }
}
