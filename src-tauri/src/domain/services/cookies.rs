// http_client/src-tauri/src/domain/services/cookies.rs
//
// Use-cases behind the cookie manager. Matching and parsing live in
// domain/cookies.rs; this only reads and removes.
use std::sync::Arc;

use crate::domain::error::AppError;
use crate::domain::models::Cookie;
use crate::domain::ports::CookieRepository;

#[derive(Clone)]
pub struct Cookies {
    cookies: Arc<dyn CookieRepository>,
}

impl Cookies {
    pub fn new(cookies: Arc<dyn CookieRepository>) -> Self {
        Self { cookies }
    }

    pub fn list(&self) -> Result<Vec<Cookie>, AppError> {
        self.cookies.list()
    }

    pub fn delete(&self, domain: &str, path: &str, name: &str) -> Result<(), AppError> {
        self.cookies.delete(domain, path, name)
    }

    pub fn clear(&self) -> Result<(), AppError> {
        self.cookies.clear()
    }
}
