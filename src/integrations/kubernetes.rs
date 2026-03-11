use crate::error::Result;

pub struct K8sClient {}

impl K8sClient {
    pub async fn new() -> Result<Self> {
        Ok(Self {})
    }
}
