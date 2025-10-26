//! OAuth2 authentication module for Google login

use std::io::{self, BufRead};
use std::fs;
use std::path::PathBuf;
use url::Url;
use rand::Rng;
use n0_snafu::Result;
use serde::{Deserialize, Serialize};
use directories::ProjectDirs;

const CLIENT_ID: &str = "402230363945-pmkrgrkkashlcdkf8oso0pptneioqn2o.apps.googleusercontent.com";
const OAUTH_SCOPES: &str = "email profile";
const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const REDIRECT_PORT: u16 = 8585;

// Backend server base URL
// const BASE_URL: &str = "http://localhost:9000";

// Backend server base URL 
const BASE_URL: &str = "https://0hepe5jz44.execute-api.us-west-2.amazonaws.com/default";

#[derive(Debug, Serialize, Deserialize)]
struct LoginWithCodeRequest {
    code: String,
    redirect_uri: String,
    login_from: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub session_id: String,
    pub is_new_registration: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct RegisterConnectionRequest {
    connection_string: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    alias: Option<String>,
    host_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterConnectionResponse {
    pub connection_string: String,
    pub registered_at: u64,
    pub alias: Option<String>,
    pub host_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeleteConnectionRequest {
    alias: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub connection_string: String,
    pub registered_at: u64,
    pub alias: Option<String>,
    pub host_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionsListResponse {
    pub connections: Vec<Connection>,
    pub count: usize,
}

/// Generate a random state token for CSRF protection
fn generate_state_token() -> String {
    let mut rng = rand::thread_rng();
    let token: [u8; 32] = rng.r#gen();
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &token)
}

/// Craft the Google OAuth2 authorization URL
fn build_auth_url(state: &str) -> Result<String> {
    let redirect_uri = format!("http://127.0.0.1:{}", REDIRECT_PORT);

    let mut url = Url::parse(GOOGLE_AUTH_URL)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to parse auth URL: {}", e)))?;

    url.query_pairs_mut()
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", OAUTH_SCOPES)
        .append_pair("state", state);

    Ok(url.to_string())
}

/// Parse the authorization code from the callback URL
fn parse_callback_url(url: &str, expected_state: &str) -> Result<String> {
    let parsed_url = Url::parse(url)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to parse callback URL: {}", e)))?;

    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut error: Option<String> = None;

    for (key, value) in parsed_url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.to_string()),
            "state" => state = Some(value.to_string()),
            "error" => error = Some(value.to_string()),
            _ => {}
        }
    }

    if let Some(err) = error {
        return Err(n0_snafu::Error::anyhow(anyhow::anyhow!("OAuth error: {}", err)));
    }

    let received_state = state.as_deref().unwrap_or("");
    if received_state != expected_state {
        return Err(n0_snafu::Error::anyhow(anyhow::anyhow!(
            "State mismatch: expected '{}', got '{}'", expected_state, received_state
        )));
    }

    code.ok_or_else(|| n0_snafu::Error::anyhow(anyhow::anyhow!("No authorization code in callback URL")))
}

/// Get the config directory for the application, creating it if it doesn't exist
fn get_config_dir() -> Result<PathBuf> {
    let proj_dirs = ProjectDirs::from("app", "freewill", "kerr")
        .ok_or_else(|| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to determine config directory")))?;

    let config_dir = proj_dirs.config_dir();

    // Create the directory if it doesn't exist
    if !config_dir.exists() {
        fs::create_dir_all(config_dir)
            .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to create config directory: {}", e)))?;
    }

    Ok(config_dir.to_path_buf())
}

/// Save session data to session.json in the config directory
fn save_session(session_data: &LoginResponse) -> Result<()> {
    let config_dir = get_config_dir()?;
    let session_file = config_dir.join("session.json");

    let json_data = serde_json::to_string_pretty(session_data)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to serialize session data: {}", e)))?;

    fs::write(&session_file, json_data)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to write session file: {}", e)))?;

    println!("Session saved to: {}", session_file.display());

    Ok(())
}

/// Load session data from session.json in the config directory
pub fn load_session() -> Result<LoginResponse> {
    let config_dir = get_config_dir()?;
    let session_file = config_dir.join("session.json");

    if !session_file.exists() {
        return Err(n0_snafu::Error::anyhow(anyhow::anyhow!(
            "No session found. Please run 'kerr login' first."
        )));
    }

    let json_data = fs::read_to_string(&session_file)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to read session file: {}", e)))?;

    let session_data: LoginResponse = serde_json::from_str(&json_data)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to parse session data: {}", e)))?;

    Ok(session_data)
}

/// Get the session ID from the saved session
pub fn get_session_id() -> Result<String> {
    let session = load_session()?;
    Ok(session.session_id)
}

/// Exchange the authorization code with the backend server
async fn exchange_code_with_server(auth_code: &str, redirect_uri: &str) -> Result<serde_json::Value> {
    let client = reqwest::Client::new();

    let request_payload = LoginWithCodeRequest {
        code: auth_code.to_string(),
        redirect_uri: redirect_uri.to_string(),
        login_from: "terminal".to_string(),
    };

    // Print curl command for debugging
    let payload_json = serde_json::to_string(&request_payload).unwrap_or_default();
    println!("\n=== DEBUG: Equivalent curl command ===");
    println!("curl -X POST '{}/login_with_code' \\", BASE_URL);
    println!("  -H 'Content-Type: application/json' \\");
    println!("  -d '{}'", payload_json);
    println!("=====================================\n");

    let response = client
        .post(format!("{}/login_with_code", BASE_URL))
        .json(&request_payload)
        .send()
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to send request to auth server: {}", e)))?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(n0_snafu::Error::anyhow(anyhow::anyhow!(
            "Auth server returned error {}: {}", status, error_text
        )));
    }

    let response_data = response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to parse server response: {}", e)))?;

    Ok(response_data)
}

/// Run the OAuth2 login flow
pub async fn login() -> Result<()> {
    println!("Starting OAuth2 login flow...");
    println!("Redirect URI is set to: http://127.0.0.1:{}", REDIRECT_PORT);

    // Generate state token for CSRF protection
    let state = generate_state_token();

    // Build the authorization URL
    let auth_url = build_auth_url(&state)?;

    println!("\n{}", "=".repeat(80));
    println!("Please visit the following URL in your browser to authenticate:");
    println!("\n{}\n", auth_url);
    println!("{}", "=".repeat(80));

    println!("\nAfter authentication, you will be redirected to a URL that looks like:");
    println!("http://127.0.0.1:{}/?state=...&code=...&scope=...", REDIRECT_PORT);
    println!("\nPlease paste the complete callback URL here:");

    // Read the callback URL from stdin
    let stdin = io::stdin();
    let mut callback_url = String::new();
    stdin.lock().read_line(&mut callback_url)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to read from stdin: {}", e)))?;

    let callback_url = callback_url.trim();

    // Parse the authorization code from the callback URL
    let auth_code = parse_callback_url(callback_url, &state)?;

    println!("\nAuthorization code received!");
    println!("Exchanging code with authentication server...");

    // Exchange the code with the backend server
    let redirect_uri = format!("http://127.0.0.1:{}", REDIRECT_PORT);
    let server_response = exchange_code_with_server(&auth_code, &redirect_uri).await?;

    println!("\nAuthentication successful!");

    // Parse the response into LoginResponse struct
    let session_data: LoginResponse = serde_json::from_value(server_response)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to parse login response: {}", e)))?;

    println!("Session ID: {}", session_data.session_id);

    // Save the session to config directory
    save_session(&session_data)?;

    Ok(())
}

/// Register a P2P connection with the backend server
pub async fn register_connection(
    connection_string: String,
    alias: Option<String>,
    host_name: String,
) -> Result<RegisterConnectionResponse> {
    let session_id = get_session_id()?;
    let client = reqwest::Client::new();

    let request_payload = RegisterConnectionRequest {
        connection_string,
        alias,
        host_name,
    };

    // Print curl command for debugging
    let payload_json = serde_json::to_string(&request_payload).unwrap_or_default();
    println!("\n=== DEBUG: /register_connection request ===");
    println!("curl -X POST '{}/register_connection' \\", BASE_URL);
    println!("  -H 'Content-Type: application/json' \\");
    println!("  -H 'kerr_session: {}' \\", session_id);
    println!("  -d '{}'", payload_json);
    println!("==========================================\n");

    let response = client
        .post(format!("{}/register_connection", BASE_URL))
        .header("kerr_session", session_id)
        .json(&request_payload)
        .send()
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to register connection: {}", e)))?;

    let status = response.status();
    let response_text = response.text().await.unwrap_or_else(|_| "Unable to read response".to_string());

    println!("\n=== DEBUG: /register_connection response ===");
    println!("Status: {}", status);
    println!("Body: {}", response_text);
    println!("============================================\n");

    if !status.is_success() {
        return Err(n0_snafu::Error::anyhow(anyhow::anyhow!(
            "Backend returned error {}: {}",
            status,
            response_text
        )));
    }

    let registration: RegisterConnectionResponse = serde_json::from_str(&response_text)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to parse registration response: {}", e)))?;

    Ok(registration)
}

/// Unregister a P2P connection from the backend server
pub async fn unregister_connection(alias: String) -> Result<()> {
    let session_id = get_session_id()?;
    let client = reqwest::Client::new();

    let request_payload = DeleteConnectionRequest {
        alias,
    };

    let response = client
        .delete(format!("{}/connection", BASE_URL))
        .header("kerr_session", session_id)
        .json(&request_payload)
        .send()
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to unregister connection: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(n0_snafu::Error::anyhow(anyhow::anyhow!(
            "Backend returned error {}: {}",
            status,
            error_text
        )));
    }

    Ok(())
}

/// Fetch all connections for the authenticated user from the backend server
pub async fn fetch_connections() -> Result<ConnectionsListResponse> {
    let session_id = get_session_id()?;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/connections", BASE_URL))
        .header("kerr_session", session_id)
        .send()
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to fetch connections: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(n0_snafu::Error::anyhow(anyhow::anyhow!(
            "Backend returned error {}: {}",
            status,
            error_text
        )));
    }

    let connections: ConnectionsListResponse = response
        .json()
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to parse connections response: {}", e)))?;

    Ok(connections)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LogoutResponse {
    pub message: String,
    pub session_id: String,
}

/// Delete the session file
fn delete_session() -> Result<()> {
    let config_dir = get_config_dir()?;
    let session_file = config_dir.join("session.json");

    if session_file.exists() {
        fs::remove_file(&session_file)
            .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to delete session file: {}", e)))?;
    }

    Ok(())
}

/// Logout and invalidate the current session
pub async fn logout() -> Result<()> {
    let session_id = get_session_id()?;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/logout", BASE_URL))
        .header("kerr_session", &session_id)
        .send()
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to logout: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(n0_snafu::Error::anyhow(anyhow::anyhow!(
            "Backend returned error {}: {}",
            status,
            error_text
        )));
    }

    let logout_response: LogoutResponse = response
        .json()
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to parse logout response: {}", e)))?;

    println!("{}", logout_response.message);
    println!("Session ID: {}", logout_response.session_id);

    // Delete the session file
    delete_session()?;
    println!("Local session file removed");

    Ok(())
}
