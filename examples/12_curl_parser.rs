//! Curl parser — parse "Copy as cURL" commands from browser DevTools.

use scrapling::shell::parse_curl;

fn main() {
    // Parse a typical DevTools "Copy as cURL" command
    let curl_cmd = r#"curl 'https://api.example.com/data' \
        -H 'Accept: application/json' \
        -H 'Authorization: Bearer tok_abc123' \
        -H 'Cookie: session=xyz789; theme=dark' \
        -H 'User-Agent: Mozilla/5.0' \
        --compressed"#;

    let request = parse_curl(curl_cmd).unwrap();

    println!("Method: {}", request.method);
    println!("URL: {}", request.url);
    println!("\nHeaders:");
    for (k, v) in &request.headers {
        println!("  {k}: {v}");
    }
    println!("\nCookies:");
    for (k, v) in &request.cookies {
        println!("  {k} = {v}");
    }

    // Parse a POST with JSON body
    let post_cmd = r#"curl -X POST 'https://api.example.com/login' \
        -H 'Content-Type: application/json' \
        -d '{"username":"admin","password":"secret"}'"#;

    let post_req = parse_curl(post_cmd).unwrap();
    println!("\n--- POST ---");
    println!("Method: {}", post_req.method);
    println!("URL: {}", post_req.url);
    println!("JSON body: {:?}", post_req.json_data);

    // Parse with proxy
    let proxy_cmd = "curl -x http://proxy:8080 -L https://example.com";
    let proxy_req = parse_curl(proxy_cmd).unwrap();
    println!("\n--- Proxy ---");
    println!("Proxy: {:?}", proxy_req.proxy);
    println!("Follow redirects: {}", proxy_req.follow_redirects);
}
