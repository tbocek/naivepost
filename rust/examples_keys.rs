fn main() {
    use naivepost::fx_record as rec;
    use naivepost::cut::EffectKind;
    for k in [EffectKind::Zoom, EffectKind::Speed, EffectKind::Text, EffectKind::Svg, EffectKind::Volume, EffectKind::Label] {
        let v = serde_json::to_value(rec::blank(k, 0.0, 2.0)).unwrap();
        println!("{:?} -> {}", k, v);
    }
}
