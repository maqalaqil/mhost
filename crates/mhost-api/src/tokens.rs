use std::path::PathBuf;

use argon2::password_hash::{rand_core::OsRng, PasswordHash, SaltString};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::roles::Role;

/// An API token stored on disk (secret is hashed, never stored raw).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToken {
    pub id: String,
    pub name: String,
    pub secret_hash: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl ApiToken {
    /// Returns true if this token has an expiry date in the past.
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| exp < Utc::now())
            .unwrap_or(false)
    }
}

/// Returned once at creation time — contains the raw secret that will never be
/// retrievable again.
#[derive(Debug)]
pub struct CreatedToken {
    pub token: ApiToken,
    pub raw_secret: String,
}

/// Manages API tokens persisted to a JSON file.
#[derive(Debug)]
pub struct TokenStore {
    path: PathBuf,
    tokens: Vec<ApiToken>,
}

#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("duplicate token name: {0}")]
    DuplicateName(String),

    #[error("token not found: {0}")]
    NotFound(String),

    #[error("password hash error: {0}")]
    Hash(String),
}

impl TokenStore {
    /// Load tokens from the given JSON file. If the file does not exist an
    /// empty store is created.
    pub fn load(path: PathBuf) -> Result<Self, TokenError> {
        let tokens = if path.exists() {
            let data = std::fs::read_to_string(&path)?;
            serde_json::from_str(&data)?
        } else {
            Vec::new()
        };
        Ok(Self { path, tokens })
    }

    /// Persist the current token list to disk.
    pub fn save(&self) -> Result<(), TokenError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(&self.tokens)?;
        std::fs::write(&self.path, data)?;
        Ok(())
    }

    /// Create a new token. The raw secret is returned once in `CreatedToken`.
    pub fn create(
        &mut self,
        name: String,
        role: Role,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<CreatedToken, TokenError> {
        if self.tokens.iter().any(|t| t.name == name) {
            return Err(TokenError::DuplicateName(name));
        }

        let id = format!("tok_{}", &Uuid::new_v4().to_string()[..8]);
        let raw_secret = format!(
            "mhost_tok_{}_{}",
            &Uuid::new_v4().to_string()[..8],
            Uuid::new_v4().simple()
        );

        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let secret_hash = argon2
            .hash_password(raw_secret.as_bytes(), &salt)
            .map_err(|e| TokenError::Hash(e.to_string()))?
            .to_string();

        let token = ApiToken {
            id,
            name,
            secret_hash,
            role,
            created_at: Utc::now(),
            last_used: None,
            expires_at,
        };

        self.tokens.push(token.clone());
        self.save()?;

        Ok(CreatedToken { token, raw_secret })
    }

    /// Verify a raw secret against all non-expired tokens. Returns a reference
    /// to the matching token if found.
    pub fn verify(&self, raw_secret: &str) -> Option<&ApiToken> {
        let argon2 = Argon2::default();
        self.tokens.iter().find(|t| {
            if t.is_expired() {
                return false;
            }
            let Ok(parsed) = PasswordHash::new(&t.secret_hash) else {
                return false;
            };
            argon2
                .verify_password(raw_secret.as_bytes(), &parsed)
                .is_ok()
        })
    }

    /// Update the `last_used` timestamp for the given token id.
    pub fn update_last_used(&mut self, id: &str) -> Result<(), TokenError> {
        let token = self
            .tokens
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| TokenError::NotFound(id.to_string()))?;
        token.last_used = Some(Utc::now());
        self.save()?;
        Ok(())
    }

    /// Return all tokens.
    pub fn list(&self) -> &[ApiToken] {
        &self.tokens
    }

    /// Revoke (delete) a token by id.
    pub fn revoke(&mut self, id: &str) -> Result<(), TokenError> {
        let idx = self
            .tokens
            .iter()
            .position(|t| t.id == id)
            .ok_or_else(|| TokenError::NotFound(id.to_string()))?;
        self.tokens.remove(idx);
        self.save()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn store_path(dir: &std::path::Path) -> PathBuf {
        dir.join("api-tokens.json")
    }

    #[test]
    fn test_create_and_verify_token() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = TokenStore::load(store_path(dir.path())).unwrap();

        let created = store
            .create("my-token".into(), Role::Admin, None)
            .unwrap();

        assert!(created.token.id.starts_with("tok_"));
        assert!(created.raw_secret.starts_with("mhost_tok_"));

        let found = store.verify(&created.raw_secret);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "my-token");
    }

    #[test]
    fn test_duplicate_name_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = TokenStore::load(store_path(dir.path())).unwrap();

        store
            .create("dup".into(), Role::Viewer, None)
            .unwrap();

        let result = store.create("dup".into(), Role::Operator, None);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), TokenError::DuplicateName(n) if n == "dup")
        );
    }

    #[test]
    fn test_revoke_token() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = TokenStore::load(store_path(dir.path())).unwrap();

        let created = store
            .create("revokable".into(), Role::Operator, None)
            .unwrap();
        let secret = created.raw_secret.clone();
        let id = created.token.id.clone();

        store.revoke(&id).unwrap();

        assert!(store.verify(&secret).is_none());
    }

    #[test]
    fn test_expired_token_not_verified() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = TokenStore::load(store_path(dir.path())).unwrap();

        let past = Utc::now() - Duration::hours(1);
        let created = store
            .create("expired".into(), Role::Admin, Some(past))
            .unwrap();

        assert!(store.verify(&created.raw_secret).is_none());
    }

    #[test]
    fn test_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = store_path(dir.path());

        let raw_secret;
        {
            let mut store = TokenStore::load(path.clone()).unwrap();
            let created = store
                .create("persist".into(), Role::Viewer, None)
                .unwrap();
            raw_secret = created.raw_secret;
        }

        // Reload from disk
        let store2 = TokenStore::load(path).unwrap();
        let found = store2.verify(&raw_secret);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "persist");
    }

    #[test]
    fn test_revoke_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = TokenStore::load(store_path(dir.path())).unwrap();

        let result = store.revoke("tok_doesnotexist");
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), TokenError::NotFound(id) if id == "tok_doesnotexist")
        );
    }
}
