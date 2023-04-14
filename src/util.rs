use async_trait::async_trait;
use secrecy::ExposeSecret;
use serde::{de, ser::Serializer, Serialize};

use crate::error::Error;

#[async_trait]
pub(crate) trait HyperExt {
    async fn deserialize<T>(self) -> Result<T, Error>
    where
        T: de::DeserializeOwned;
}

#[async_trait]
impl HyperExt for hyper::Response<hyper::body::Body> {
    async fn deserialize<T>(self) -> Result<T, Error>
    where
        T: de::DeserializeOwned,
    {
        let (parts, body) = self.into_parts();
        let body = hyper::body::to_bytes(body)
            .await
            .map_err(Error::ConnectionError)?;

        if !parts.status.is_success() {
            let error = format!(
                "Server responded with error {}: {}",
                parts.status,
                String::from_utf8_lossy(body.as_ref())
            );
            tracing::error!("{}", error);
            return Err(Error::ServerUnavailable(error));
        }

        let token = serde_json::from_slice(&body).map_err(Error::ParsingError)?;
        Ok(token)
    }
}

/// Used in conjunction with the [`secrecy`] crate to serialize secrets.
pub(crate) fn serialize_secret<Ser: Serializer>(
    secret: &secrecy::SecretString,
    serializer: Ser,
) -> Result<Ser::Ok, Ser::Error> {
    secret.expose_secret().serialize(serializer)
}
