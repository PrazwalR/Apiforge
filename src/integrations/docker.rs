use crate::error::Result;

pub struct DockerClient {}

impl DockerClient {
    pub async fn new() -> Result<Self> {
        Ok(Self {})
    }
}
