use crate::error::Result;

pub struct AwsClient {}

impl AwsClient {
    pub async fn new() -> Result<Self> {
        Ok(Self {})
    }
}
