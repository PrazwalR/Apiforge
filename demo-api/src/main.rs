use std::env;

fn main() {
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "unknown".to_string());
    println!("🚀 Demo API v{} is running!", version);
    println!("✅ Health check: OK");
    println!("📦 Built with Apiforge automation");
}
