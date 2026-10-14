fn main() {
    use naivepost::{cut::{Fx, Seg}, cut_speed, render_fx};
    let seg = Seg { s: 0.0, e: 20.0, ..Default::default() };
    for (dur, rate, ramp) in [(12.0, 8.0, 6.0), (12.0, 8.0, 0.5), (10.0, 500.0, 4.0)] {
        let fx = vec![Fx { kind: "speed".into(), t: 0.0, dur, rate, trans: ramp, tout: ramp, ..Default::default() }];
        println!("rate_spans({rate} ramp {ramp}) = {:?}", cut_speed::rate_spans(&fx));
        println!("split_pictures = {:?}", render_fx::split_pictures(&fx, &seg));
    }
}
