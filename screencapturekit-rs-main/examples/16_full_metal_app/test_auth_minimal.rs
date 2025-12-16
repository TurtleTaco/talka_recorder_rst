// Minimal test of the auth callback server
// Run with: cargo run --bin test_auth_minimal

use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use warp::Filter;

#[tokio::main]
async fn main() {
    println!("🧪 Minimal Auth Callback Server Test");
    println!("====================================\n");

    let (tx, rx) = oneshot::channel();
    let tx_wrapped = Arc::new(Mutex::new(Some(tx)));

    // Health check route
    let health = warp::path("health").map(|| {
        println!("✅ Health check received");
        warp::reply::html("OK - Health Check")
    });

    // Debug route that catches everything
    let debug_route = warp::any()
        .and(warp::path::full())
        .and(warp::query::<std::collections::HashMap<String, String>>())
        .map(|path: warp::path::FullPath, params: std::collections::HashMap<String, String>| {
            println!("🔍 Request received:");
            println!("   Path: {}", path.as_str());
            println!("   Query params: {:?}", params);
            warp::reply::html(format!("Request received: {}", path.as_str()))
        });

    // Callback route (specific path)
    let callback_route = warp::get()
        .and(warp::path!("login" / "oauth2" / "code" / "oidc"))
        .and(warp::query::<std::collections::HashMap<String, String>>())
        .map(move |params: std::collections::HashMap<String, String>| {
            println!("🎉 CALLBACK ROUTE MATCHED!");
            println!("   Params: {:?}", params);
            
            if let Some(code) = params.get("code") {
                println!("   ✅ Authorization code: {}", code);
                
                // Send success signal
                if let Ok(mut tx_guard) = tx_wrapped.lock() {
                    if let Some(tx) = tx_guard.take() {
                        let _ = tx.send(code.clone());
                    }
                }
            }
            
            warp::reply::html(r#"
                <!DOCTYPE html>
                <html>
                <head><title>Callback Received</title></head>
                <body>
                    <h1>✅ Callback Received!</h1>
                    <p>Check the terminal for details.</p>
                </body>
                </html>
            "#)
        });

    // Combine routes - specific first, then catch-all
    let routes = callback_route.or(health).or(debug_route);

    println!("🔧 Starting server on http://127.0.0.1:8080");
    println!("📍 Test URLs:");
    println!("   http://127.0.0.1:8080/health");
    println!("   http://127.0.0.1:8080/login/oauth2/code/oidc?code=test123&state=xyz");
    println!("\n⏳ Server running... (Ctrl+C to stop)");
    println!("   Waiting for callback on /login/oauth2/code/oidc\n");

    // Start server
    let server_handle = tokio::spawn(warp::serve(routes).bind(([127, 0, 0, 1], 8080)));

    // Wait for callback or timeout
    tokio::select! {
        result = rx => {
            match result {
                Ok(code) => {
                    println!("\n🎉 SUCCESS! Received authorization code: {}", code);
                    println!("✅ The callback route is working correctly!");
                }
                Err(_) => {
                    println!("\n❌ Channel closed without receiving code");
                }
            }
            server_handle.abort();
        }
        _ = tokio::time::sleep(tokio::time::Duration::from_secs(300)) => {
            println!("\n⏰ Timeout after 5 minutes");
            server_handle.abort();
        }
    }
}

