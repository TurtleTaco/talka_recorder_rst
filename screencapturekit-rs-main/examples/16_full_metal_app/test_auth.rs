//! Standalone test for Auth0 Device Authorization Flow
//!
//! Run with: cargo run --example 16_full_metal_app --bin test_auth --features macos_15_0
//! Or directly: rustc test_auth.rs && ./test_auth

mod auth;

#[tokio::main]
async fn main() {
    println!("╔════════════════════════════════════════════════════╗");
    println!("║  Auth0 Device Authorization Flow - Test Suite     ║");
    println!("╚════════════════════════════════════════════════════╝\n");

    // Test the complete flow
    match auth::complete_device_flow().await {
        Ok(tokens) => {
            println!("\n✅ Authentication test PASSED!\n");
            println!("Token Details:");
            println!("─────────────────────────────────────────────────");
            println!("Token Type:    {}", tokens.token_type);
            println!("Expires In:    {} seconds ({} hours)", 
                tokens.expires_in, 
                tokens.expires_in / 3600
            );
            println!("\nAccess Token (first 50 chars):");
            println!("  {}", &tokens.access_token.chars().take(50).collect::<String>());
            
            if !tokens.id_token.is_empty() {
                println!("\nID Token (first 50 chars):");
                println!("  {}", &tokens.id_token.chars().take(50).collect::<String>());
            }
            
            if !tokens.refresh_token.is_empty() {
                println!("\n🔄 Refresh Token Available: Yes");
            } else {
                println!("\n🔄 Refresh Token Available: No");
            }

            println!("\n─────────────────────────────────────────────────");
            println!("💡 You can now use this access token to make API calls:");
            println!("   curl -H 'Authorization: Bearer <token>' https://api.talka.ai/endpoint");
            println!("\n✅ All tests passed!");
        }
        Err(e) => {
            eprintln!("\n❌ Authentication test FAILED!");
            eprintln!("Error: {}\n", e);
            std::process::exit(1);
        }
    }
}

