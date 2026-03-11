use crate::error::Result;

pub struct GitHubClient {}

impl GitHubClient {
    pub async fn new() -> Result<Self> {
        Ok(Self {})
    }
}
