pub mod admin;
pub mod auth_routes;
pub mod duck;
pub mod flock;
pub mod me;
pub mod public;

use std::collections::HashMap;

use auth::AuthenticatedUser;
use axum::extract::Multipart;
use bytes::Bytes;

use crate::error::WebError;
use crate::state::AppState;
use crate::views::Nav;

/// Build the page chrome context for an optional viewer.
pub async fn nav(state: &AppState, user: Option<&AuthenticatedUser>) -> Result<Nav, WebError> {
    match user {
        None => Ok(Nav::anonymous()),
        Some(user) => Ok(Nav {
            display_name: user.display_name.clone(),
            logged_in: true,
            is_admin: user.is_admin,
            unread: state.notifications().unread_count(user.id).await?,
        }),
    }
}

/// Collected multipart form: text fields and file fields, small enough to
/// buffer (the body limit layer bounds the total).
pub struct FormData {
    texts: HashMap<String, String>,
    files: HashMap<String, Bytes>,
}

impl FormData {
    pub async fn read(mut multipart: Multipart) -> Result<Self, WebError> {
        let mut texts = HashMap::new();
        let mut files = HashMap::new();
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|e| WebError::BadRequest(e.to_string()))?
        {
            let name = field.name().unwrap_or_default().to_owned();
            match field.file_name().is_some() {
                true => {
                    let bytes = field
                        .bytes()
                        .await
                        .map_err(|e| WebError::BadRequest(e.to_string()))?;
                    if !bytes.is_empty() {
                        files.insert(name, bytes);
                    }
                }
                false => {
                    let text = field
                        .text()
                        .await
                        .map_err(|e| WebError::BadRequest(e.to_string()))?;
                    texts.insert(name, text);
                }
            }
        }
        Ok(Self { texts, files })
    }

    pub fn text(&self, name: &str) -> Option<&str> {
        self.texts.get(name).map(String::as_str).filter(|t| !t.trim().is_empty())
    }

    pub fn require_text(&self, name: &str) -> Result<&str, WebError> {
        self.text(name)
            .ok_or_else(|| WebError::BadRequest(format!("missing field {name}")))
    }

    pub fn file(&self, name: &str) -> Option<&Bytes> {
        self.files.get(name)
    }
}
