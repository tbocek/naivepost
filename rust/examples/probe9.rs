use naivepost::llm_liveness as live;
fn main() {
    let mut g = live::Gate::new("llm", 1);
    println!("a {:?}", g.acquire("publish"));
    println!("b {:?}", g.acquire("narrate"));
    println!("holders={:?} waiting={:?}", g.holders(), g.waiting());
    g.release("publish");
    println!("after release holders={:?} waiting={:?}", g.holders(), g.waiting());
    println!("--- shares:");
    for (a,b) in [("http://llama:8080","http://audiocpp:8765"),("http://127.0.0.1:8731","http://127.0.0.1:8765"),("llama:8080","http://llama:8080/"),("http://host","http://host:80/sd")] {
        println!("{a} vs {b} = {}", live::shares_a_server(a,b));
    }
    println!("--- blame after bytes:");
    let mut w = live::Watch::streamed("publish", 0);
    w.answered_at(120, "{\"title\": \"x\"}", false);
    println!("{:?}", w.poll(540));
}
