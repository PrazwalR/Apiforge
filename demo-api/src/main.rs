use std::env;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() {
    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{}", port);

    println!("🚀 Demo API v{} starting on {}", VERSION, addr);

    let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
    println!("✅ Server listening on {}", addr);

    loop {
        let (mut socket, _) = listener.accept().await.expect("Failed to accept");

        tokio::spawn(async move {
            let mut buf = [0; 1024];
            let _ = socket.read(&mut buf).await;

            let request = String::from_utf8_lossy(&buf);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");

            let (status, body) = match path {
                "/health" | "/healthz" => ("200 OK", format!(r#"{{"status":"healthy","version":"{}"}}"#, VERSION)),
                "/" => ("200 OK", format!("Demo API v{} - Built with Apiforge", VERSION)),
                _ => ("404 Not Found", "Not Found".to_string()),
            };

            let response = format!(
                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                status,
                body.len(),
                body
            );

            let _ = socket.write_all(response.as_bytes()).await;
        });
    }
}
