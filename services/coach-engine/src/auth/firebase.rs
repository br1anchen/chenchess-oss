use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use jsonwebtoken::jwk::JwkSet;
use reqwest::Client;
use tokio::sync::Mutex;

use super::{
    keys::{matching_keys, verification_keys, VerificationKey},
    AuthError,
};

const FIREBASE_JWKS_URL: &str =
    "https://www.googleapis.com/service_accounts/v1/jwk/securetoken@system.gserviceaccount.com";
const DEFAULT_CACHE_SECONDS: u64 = 300;
const MAX_CACHE_SECONDS: u64 = 86_400;
const STALE_KEY_GRACE_SECONDS: u64 = 300;
const FIREBASE_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const FIREBASE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub(super) struct FirebaseKeySource {
    source: Arc<KeySource>,
}

enum KeySource {
    Static(Vec<VerificationKey>),
    Remote(RemoteFirebaseKeys),
}

struct RemoteFirebaseKeys {
    client: Client,
    cache: Mutex<CachedKeys>,
}

struct CachedKeys {
    keys: Vec<VerificationKey>,
    fresh_until: Instant,
    stale_until: Instant,
}

impl FirebaseKeySource {
    pub(super) fn static_keys(keys: Vec<VerificationKey>) -> Self {
        Self {
            source: Arc::new(KeySource::Static(keys)),
        }
    }

    pub(super) fn remote() -> anyhow::Result<Self> {
        let now = Instant::now();
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(FIREBASE_CONNECT_TIMEOUT)
            .timeout(FIREBASE_RESPONSE_TIMEOUT)
            .build()?;
        Ok(Self {
            source: Arc::new(KeySource::Remote(RemoteFirebaseKeys {
                client,
                cache: Mutex::new(CachedKeys {
                    keys: Vec::new(),
                    fresh_until: now,
                    stale_until: now,
                }),
            })),
        })
    }

    pub(super) async fn candidates(&self, kid: &str) -> Result<Vec<VerificationKey>, AuthError> {
        match self.source.as_ref() {
            KeySource::Static(keys) => Ok(matching_keys(keys, kid)),
            KeySource::Remote(remote) => remote.candidates(kid).await,
        }
    }
}

impl RemoteFirebaseKeys {
    async fn candidates(&self, kid: &str) -> Result<Vec<VerificationKey>, AuthError> {
        let mut cache = self.cache.lock().await;
        let now = Instant::now();
        let cached_match = matching_keys(&cache.keys, kid);
        // A fresh Google key set is authoritative. Unknown kids must not turn
        // unauthenticated requests into an unbounded JWKS refresh channel.
        if now < cache.fresh_until {
            return Ok(cached_match);
        }

        match self.fetch().await {
            Ok((keys, lifetime)) => {
                cache.keys = keys;
                let now = Instant::now();
                cache.fresh_until = now + lifetime;
                cache.stale_until =
                    cache.fresh_until + Duration::from_secs(STALE_KEY_GRACE_SECONDS);
            }
            Err(()) if !cached_match.is_empty() && now < cache.stale_until => {
                return Ok(cached_match)
            }
            Err(()) => return Err(AuthError::InvalidToken),
        }
        Ok(matching_keys(&cache.keys, kid))
    }

    async fn fetch(&self) -> Result<(Vec<VerificationKey>, Duration), ()> {
        let response = self
            .client
            .get(FIREBASE_JWKS_URL)
            .send()
            .await
            .map_err(|_| ())?;
        if !response.status().is_success() {
            return Err(());
        }
        let lifetime = response
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok())
            .and_then(cache_max_age)
            .unwrap_or(Duration::from_secs(DEFAULT_CACHE_SECONDS))
            .min(Duration::from_secs(MAX_CACHE_SECONDS));
        let jwks: JwkSet = response.json().await.map_err(|_| ())?;
        let keys = verification_keys(&jwks).map_err(|_| ())?;
        Ok((keys, lifetime))
    }
}

fn cache_max_age(value: &str) -> Option<Duration> {
    value.split(',').find_map(|directive| {
        directive
            .trim()
            .strip_prefix("max-age=")?
            .parse::<u64>()
            .ok()
            .map(Duration::from_secs)
    })
}

#[cfg(test)]
mod tests {
    use super::cache_max_age;
    use std::time::Duration;

    #[test]
    fn cache_control_uses_the_firebase_key_lifetime() {
        assert_eq!(
            cache_max_age("public, max-age=1234, must-revalidate"),
            Some(Duration::from_secs(1234))
        );
    }
}
