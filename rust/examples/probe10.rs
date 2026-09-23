use naivepost::llm_liveness as live;
fn main() {
    println!("a={:?}", "http://llama:8080");
    println!("shared llama/audiocpp = {}", live::shares_a_server("http://llama:8080", "http://audiocpp:8765"));
    println!("empty/host = {}", live::shares_a_server("", "http://host:80/sd"));
    println!("127 two ports = {}", live::shares_a_server("http://127.0.0.1:8731", "http://127.0.0.1:8765"));
    println!("same host+port, two prefixes = {}", live::shares_a_server("http://127.0.0.1:8731", "http://127.0.0.1:8731/v1"));
    println!("default port = {}", live::shares_a_server("http://host", "http://host:80/sd"));
}
