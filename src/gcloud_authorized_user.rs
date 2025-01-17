use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::RwLock;

use async_trait::async_trait;
use std::time::Duration;

use crate::authentication_manager::{ServiceAccount, TokenStyle};
use crate::error::Error;
use crate::error::Error::{GCloudError, GCloudParseError};
use crate::types::{HyperClient, SecretString};
use crate::Token;

/// The default number of seconds that it takes for a Google Cloud auth token to expire.
/// This appears to be the default from practical testing, but we have not found evidence
/// that this will always be the default duration.
pub(crate) const DEFAULT_TOKEN_DURATION: Duration = Duration::from_secs(3600);

#[derive(Debug)]
pub(crate) struct GCloudAuthorizedUser {
    gcloud: PathBuf,
    project_id: Option<String>,
    token: RwLock<Token>,
}

impl GCloudAuthorizedUser {
    pub(crate) async fn new() -> Result<Self, Error> {
        let gcloud = PathBuf::from("gcloud");
        let project_id = run(&gcloud, &["config", "get-value", "project"]).ok();
        let token = RwLock::new(Self::token(&gcloud)?);
        Ok(Self {
            gcloud,
            project_id,
            token,
        })
    }

    fn token(gcloud: &Path) -> Result<Token, Error> {
        Ok(Token::from_string(
            SecretString::from(run(gcloud, &["auth", "print-access-token", "--quiet"])?),
            DEFAULT_TOKEN_DURATION,
        ))
    }
}

#[async_trait]
impl ServiceAccount for GCloudAuthorizedUser {
    fn get_style(&self) -> TokenStyle {
        TokenStyle::Account
    }

    async fn project_id(&self, _: &HyperClient) -> Result<String, Error> {
        self.project_id.clone().ok_or(Error::NoProjectId)
    }

    fn get_token(&self, _scopes: &[&str]) -> Option<Token> {
        Some(self.token.read().unwrap().clone())
    }

    async fn refresh_token(&self, _client: &HyperClient, _scopes: &[&str]) -> Result<Token, Error> {
        let token = Self::token(&self.gcloud)?;
        *self.token.write().unwrap() = token.clone();
        Ok(token)
    }
}

fn run(gcloud: &Path, cmd: &[&str]) -> Result<String, Error> {
    let mut command = Command::new(gcloud);
    command.args(cmd);

    let mut stdout = match command.output() {
        Ok(output) if output.status.success() => output.stdout,
        _ => return Err(GCloudError),
    };

    while let Some(b' ' | b'\r' | b'\n') = stdout.last() {
        stdout.pop();
    }

    String::from_utf8(stdout).map_err(|_| GCloudParseError)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;

    #[tokio::test]
    #[ignore]
    async fn gcloud() {
        let gcloud = GCloudAuthorizedUser::new().await.unwrap();
        println!("{:?}", gcloud.project_id);
        if let Some(t) = gcloud.get_token(&[""]) {
            let expires = SystemTime::now() + DEFAULT_TOKEN_DURATION;
            println!("{:?}", t);
            assert!(!t.has_expired());
            assert!(t.expires_at() < expires + Duration::from_secs(1));
            assert!(t.expires_at() > expires - Duration::from_secs(1));
        } else {
            panic!("GCloud Authorized User failed to get a token");
        }
    }

    /// `gcloud_authorized_user` is the only user type to get a token that isn't deserialized from
    /// JSON, and that doesn't include an expiry time. As such, the default token expiry time
    /// functionality is tested here.
    #[test]
    fn test_token_from_string() {
        let s = SecretString::from(String::from("abc123"));
        let token = Token::from_string(s, DEFAULT_TOKEN_DURATION);
        let expires = SystemTime::now() + DEFAULT_TOKEN_DURATION;

        assert_eq!(token.secret(), "abc123");
        assert!(!token.has_expired());
        assert!(token.expires_at() < expires + Duration::from_secs(1));
        assert!(token.expires_at() > expires - Duration::from_secs(1));
    }

    #[test]
    fn test_deserialise_no_time() {
        let s = r#"{"access_token":"abc123"}"#;
        let result = serde_json::from_str::<Token>(s)
            .expect_err("Deserialization from JSON should fail when no expiry_time is included");

        assert!(result.is_data());
    }
}
