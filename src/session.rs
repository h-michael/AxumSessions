use crate::{AxumDatabasePool, AxumSessionData, AxumSessionID, AxumSessionStore, CookiesExt};
use async_trait::async_trait;
use axum_core::extract::{FromRequest, RequestParts};
use cookie::CookieJar;
use http::{self, StatusCode};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fmt::Debug,
    marker::{Send, Sync},
};
use uuid::Uuid;

/// A Session Store.
///
/// Provides a Storage Handler to AxumSessionStore and contains the AxumSessionID(UUID) of the current session.
///
/// This is Auto generated by the Session Layer Upon Service Execution.
#[derive(Debug, Clone)]
pub struct AxumSession<T>
where
    T: AxumDatabasePool + Clone + Debug + Sync + Send + 'static,
{
    pub(crate) store: AxumSessionStore<T>,
    pub(crate) id: AxumSessionID,
}

/// Adds FromRequest<B> for AxumSession
///
/// Returns the AxumSession from Axums request extensions.
#[async_trait]
impl<B, T> FromRequest<B> for AxumSession<T>
where
    B: Send,
    T: AxumDatabasePool + Clone + Debug + Sync + Send + 'static,
{
    type Rejection = (http::StatusCode, &'static str);

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        req.extensions().get::<AxumSession<T>>().cloned().ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Can't extract AxumSession. Is `AxumSessionLayer` enabled?",
        ))
    }
}

impl<S> AxumSession<S>
where
    S: AxumDatabasePool + Clone + Debug + Sync + Send + 'static,
{
    pub(crate) fn new(store: &AxumSessionStore<S>, cookies: &CookieJar) -> AxumSession<S> {
        let value = cookies
            .get_cookie(&store.config.cookie_name, &store.config.key)
            .and_then(|c| Uuid::parse_str(c.value()).ok());

        let uuid = match value {
            Some(v) => v,
            None => loop {
                let token = Uuid::new_v4();

                if !store.inner.contains_key(&token.to_string()) {
                    break token;
                }
            },
        };

        AxumSession {
            id: AxumSessionID(uuid),
            store: store.clone(),
        }
    }
    /// Runs a Closure upon the Current Sessions stored data to get or set session data.
    ///
    /// Provides an Option<T> that returns the requested data from the Sessions store.
    ///
    /// # Examples
    /// ```rust no_run
    /// session.tap(|sess| {
    ///   let string = sess.data.get(key)?;
    ///   serde_json::from_str(string).ok()
    /// }).await;
    /// ```
    ///
    pub(crate) fn tap<T: DeserializeOwned>(
        &self,
        func: impl FnOnce(&mut AxumSessionData) -> Option<T>,
    ) -> Option<T> {
        if let Some(mut instance) = self.store.inner.get_mut(&self.id.0.to_string()) {
            func(&mut instance)
        } else {
            tracing::warn!("Session data unexpectedly missing");
            None
        }
    }

    /// Sets the Current Session to be Destroyed on the next run.
    ///
    /// # Examples
    /// ```rust no_run
    /// session.destroy().await;
    /// ```
    ///
    pub async fn destroy(&self) {
        self.tap(|sess| {
            sess.destroy = true;
            Some(1)
        });
    }

    /// Sets the Current Session to a long term expiration. Useful for Remember Me setups.
    ///
    /// # Examples
    /// ```rust no_run
    /// session.set_longterm(true).await;
    /// ```
    ///
    pub async fn set_longterm(&self, longterm: bool) {
        self.tap(|sess| {
            sess.longterm = longterm;
            Some(1)
        });
    }

    /// Sets the Current Session to be storable.
    ///
    /// This will allow the Session to save its data for the lifetime if set to true.
    /// If this is set to false it will unload the stored session.
    ///
    /// # Examples
    /// ```rust no_run
    /// session.set_store(true).await;
    /// ```
    ///
    pub async fn set_store(&self, storable: bool) {
        self.tap(|sess| {
            sess.storable = storable;
            Some(1)
        });
    }

    /// Gets data from the Session's HashMap
    ///
    /// Provides an Option<T> that returns the requested data from the Sessions store.
    /// Returns None if Key does not exist or if serdes_json failed to deserialize.
    ///
    /// # Examples
    /// ```rust no_run
    /// let id = session.get("user-id").await.unwrap_or(0);
    /// ```
    ///
    ///Used to get data stored within SessionDatas hashmap from a key value.
    pub async fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.tap(|sess| {
            let string = sess.data.get(key)?;
            serde_json::from_str(string).ok()
        })
    }

    /// Sets data to the Current Session's HashMap.
    ///
    /// # Examples
    /// ```rust no_run
    /// session.set("user-id", 1).await;
    /// ```
    ///
    pub async fn set(&self, key: &str, value: impl Serialize) {
        let value = serde_json::to_string(&value).unwrap_or_else(|_| "".to_string());

        self.tap(|sess| {
            if sess.data.get(key) != Some(&value) {
                sess.data.insert(key.to_string(), value);
            }
            Some(1)
        });
    }

    /// Removes a Key from the Current Session's HashMap.
    ///
    /// # Examples
    /// ```rust no_run
    /// session.remove("user-id").await;
    /// ```
    ///
    pub async fn remove(&self, key: &str) {
        self.tap(|sess| sess.data.remove(key));
    }

    /// Clears all data from the Current Session's HashMap.
    ///
    /// # Examples
    /// ```rust no_run
    /// session.clear_all().await;
    /// ```
    ///
    pub async fn clear_all(&self) {
        if let Some(mut instance) = self.store.inner.get_mut(&self.id.0.to_string()) {
            instance.data.clear();
        }

        if self.store.is_persistent() {
            self.store.clear_store().await.unwrap();
        }
    }

    /// Returns a i64 count of how many Sessions exist.
    ///
    /// If the Session is persistant it will return all sessions within the database.
    /// If the Session is not persistant it will return a count within AxumSessionStore.
    ///
    /// # Examples
    /// ```rust no_run
    /// let count = session.count().await;
    /// ```
    ///
    pub async fn count(&self) -> i64 {
        if self.store.is_persistent() {
            self.store.count().await.unwrap_or(0i64)
        } else {
            self.store.inner.len() as i64
        }
    }
}
