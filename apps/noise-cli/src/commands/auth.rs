//! CLI authentication commands.
//!
//! Usage:
//!   noise auth login    --username alice --password hunter2 --server http://localhost:8080
//!   noise auth logout
//!   noise auth whoami
//!   noise auth register --username bob --email bob@b.com --password pw [--role viewer]

use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Path to the credential cache file.
fn credentials_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".noise").join("credentials.json")
}

/// Cached credentials stored on disk.
#[derive(Debug, Serialize, Deserialize)]
struct Credentials {
    token: String,
    username: String,
    role: String,
    server: String,
}

fn load_credentials() -> Option<Credentials> {
    let path = credentials_path();
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn save_credentials(creds: &Credentials) -> anyhow::Result<()> {
    let path = credentials_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(creds)?)?;
    Ok(())
}

fn delete_credentials() {
    let _ = std::fs::remove_file(credentials_path());
}

// ─── Clap types ──────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub action: AuthAction,
}

#[derive(Subcommand)]
pub enum AuthAction {
    /// Log in and save a JWT token to ~/.noise/credentials.json.
    Login {
        #[arg(short, long)]
        username: String,
        #[arg(short, long)]
        password: String,
        /// API server base URL.
        #[arg(long, default_value = "http://localhost:8080")]
        server: String,
    },
    /// Remove saved credentials.
    Logout,
    /// Show the currently saved identity.
    Whoami,
    /// Register a new user on the server (requires admin token or open bootstrap).
    Register {
        #[arg(long)]
        username: String,
        #[arg(long)]
        email: String,
        #[arg(long)]
        password: String,
        #[arg(long, default_value = "viewer")]
        role: String,
        #[arg(long, default_value = "http://localhost:8080")]
        server: String,
    },
    /// Verify a JWT token (local or against the server).
    Verify {
        /// Token to verify (defaults to the saved token).
        #[arg(long)]
        token: Option<String>,
    },
}

// ─── Handlers ────────────────────────────────────────────────────────────────

pub async fn run(args: AuthArgs) -> anyhow::Result<()> {
    match args.action {
        AuthAction::Login { username, password, server } => {
            do_login(&username, &password, &server).await
        }
        AuthAction::Logout => {
            delete_credentials();
            println!("Logged out — credentials removed.");
            Ok(())
        }
        AuthAction::Whoami => {
            match load_credentials() {
                Some(c) => {
                    println!("Logged in as : {}", c.username);
                    println!("Role         : {}", c.role);
                    println!("Server       : {}", c.server);
                    println!("Token (first 20 chars): {}…", &c.token[..c.token.len().min(20)]);
                }
                None => println!("Not logged in. Run `noise auth login` first."),
            }
            Ok(())
        }
        AuthAction::Register { username, email, password, role, server } => {
            do_register(&username, &email, &password, &role, &server).await
        }
        AuthAction::Verify { token } => {
            let tok = token
                .or_else(|| load_credentials().map(|c| c.token))
                .ok_or_else(|| anyhow::anyhow!(
                    "No token provided and no saved credentials found."
                ))?;
            do_verify(&tok)
        }
    }
}

/// Login via local `AuthService` + `verify_password` — no network required.
/// This lets the CLI work offline when the DB is local.
async fn do_login(username: &str, password: &str, server: &str) -> anyhow::Result<()> {
    use noise_auth::{AuthService, TokenService};
    use noise_data::{db::Database, repository::UserRepository};

    let db_path = std::env::var("NOISE_DB").unwrap_or_else(|_| "noise.db".into());

    if std::path::Path::new(&db_path).exists() {
        // Local DB available — authenticate directly.
        let db = Database::open(&db_path)?;
        let repo = UserRepository::new(db.connection());
        let user = repo.get_by_username(username)
            .map_err(|_| anyhow::anyhow!("User '{}' not found.", username))?;

        let secret = std::env::var("NOISE_JWT_SECRET")
            .unwrap_or_else(|_| "change-me-in-production".into());
        let svc = AuthService::new(secret.as_bytes());
        let uid = uuid::Uuid::parse_str(&user.id)
            .unwrap_or_else(|_| uuid::Uuid::new_v4());
        let token = svc.login(username, password, &user.password_hash, uid, &user.role)
            .map_err(|_| anyhow::anyhow!("Invalid password."))?;

        repo.update_last_login(&user.id)?;

        let creds = Credentials {
            token,
            username: user.username.clone(),
            role: user.role.clone(),
            server: server.into(),
        };
        save_credentials(&creds)?;
        println!("Logged in as '{}' ({})", user.username, user.role);
        println!("Token saved to {}", credentials_path().display());
    } else {
        // No local DB — instruct user to start the server.
        println!("No local database found at '{db_path}'.");
        println!("Start the server first or set NOISE_DB to the correct path.");
        println!("For a demo token (offline), use `noise auth login --demo`.");
    }
    Ok(())
}

async fn do_register(
    username: &str,
    email: &str,
    password: &str,
    role: &str,
    _server: &str,
) -> anyhow::Result<()> {
    use noise_auth::{AuthService, RegisterRequest, validate_register};
    use noise_data::{db::Database, repository::{StoredUser, UserRepository}};

    let req = RegisterRequest {
        username: username.into(),
        email: email.into(),
        password: password.into(),
        role: role.into(),
    };
    validate_register(&req)
        .map_err(|e| anyhow::anyhow!("Validation error: {e}"))?;

    let db_path = std::env::var("NOISE_DB").unwrap_or_else(|_| "noise.db".into());
    let db = Database::open(&db_path)?;
    let repo = UserRepository::new(db.connection());

    let total = repo.list()?.len();
    let effective_role = if total == 0 { "admin" } else { role };

    let secret = std::env::var("NOISE_JWT_SECRET")
        .unwrap_or_else(|_| "change-me-in-production".into());
    let svc = AuthService::new(secret.as_bytes());
    let hash = svc.hash_new_password(password)?;
    let user = StoredUser::new(username, hash, email, effective_role);
    repo.insert(&user)
        .map_err(|e| anyhow::anyhow!("Registration failed: {e}"))?;

    println!("User '{}' registered with role '{}'.", username, effective_role);
    if total == 0 {
        println!("(First user — granted admin automatically.)");
    }
    Ok(())
}

fn do_verify(token: &str) -> anyhow::Result<()> {
    use noise_auth::AuthService;
    let secret = std::env::var("NOISE_JWT_SECRET")
        .unwrap_or_else(|_| "change-me-in-production".into());
    let svc = AuthService::new(secret.as_bytes());
    match svc.verify_token(token) {
        Ok(claims) => {
            println!("Token valid");
            println!("  Subject  : {}", claims.sub);
            println!("  Username : {}", claims.username);
            println!("  Role     : {}", claims.role);
            let exp = std::time::UNIX_EPOCH +
                std::time::Duration::from_secs(claims.exp);
            let secs_left = exp
                .duration_since(std::time::SystemTime::now())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            println!("  Expires  : in {}s", secs_left);
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Token invalid: {e}"));
        }
    }
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_path_is_in_home_dir() {
        let p = credentials_path();
        let s = p.to_string_lossy();
        assert!(s.contains(".noise"), "path = {s}");
        assert!(s.ends_with("credentials.json"), "path = {s}");
    }

    #[test]
    fn save_and_load_credentials() {
        let dir = std::env::temp_dir().join("noise_test_creds");
        std::env::set_var("HOME", &dir);
        let creds = Credentials {
            token: "test.token.here".into(),
            username: "alice".into(),
            role: "analyst".into(),
            server: "http://localhost:8080".into(),
        };
        save_credentials(&creds).unwrap();
        let loaded = load_credentials().unwrap();
        assert_eq!(loaded.username, "alice");
        assert_eq!(loaded.role, "analyst");
        delete_credentials();
        assert!(load_credentials().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn verify_invalid_token_exits_with_message() {
        let result = do_verify("not.a.valid.jwt");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Token invalid"), "unexpected message: {msg}");
    }
}
