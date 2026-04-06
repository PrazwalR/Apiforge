use crate::error::{AwsError, Result};
use crate::utils::{RetryConfig, RetryableError, with_retry};
use aws_config::BehaviorVersion;
use aws_sdk_ecr::Client as EcrClient;
use aws_sdk_ecr::error::SdkError;
use aws_sdk_sts::Client as StsClient;
use base64::Engine;
use bollard::auth::DockerCredentials;

/// Wrapper for AWS errors that implements RetryableError
#[derive(Debug)]
struct AwsRetryableError(AwsError);

impl std::fmt::Display for AwsRetryableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl RetryableError for AwsRetryableError {
    fn is_retryable(&self) -> bool {
        match &self.0 {
            // These are transient errors that can be retried
            AwsError::SdkError(msg) => {
                // Retry on common transient errors
                msg.contains("ThrottlingException")
                    || msg.contains("RequestTimeout")
                    || msg.contains("ServiceUnavailable")
                    || msg.contains("InternalServiceError")
                    || msg.contains("connection")
                    || msg.contains("timeout")
            }
            // Auth failures should not be retried
            AwsError::CredentialsInvalid => false,
            AwsError::EcrAuthFailed(_) => false,
            AwsError::PermissionDenied(_) => false,
            // Repo not found is a permanent error
            AwsError::EcrRepoNotFound(_) => false,
            AwsError::RegionNotConfigured => false,
        }
    }
}

impl From<AwsRetryableError> for crate::error::ApiForgError {
    fn from(e: AwsRetryableError) -> Self {
        crate::error::ApiForgError::Aws(e.0)
    }
}

pub struct AwsClient {
    ecr: EcrClient,
    sts: StsClient,
    region: String,
    retry_config: RetryConfig,
}

impl AwsClient {
    pub async fn new(region: &str) -> Result<Self> {
        let config = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_config::Region::new(region.to_string()))
            .load()
            .await;

        let ecr = EcrClient::new(&config);
        let sts = StsClient::new(&config);

        Ok(Self {
            ecr,
            sts,
            region: region.to_string(),
            retry_config: RetryConfig::default(),
        })
    }

    pub async fn with_profile(region: &str, profile: &str) -> Result<Self> {
        let config = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_config::Region::new(region.to_string()))
            .profile_name(profile)
            .load()
            .await;

        let ecr = EcrClient::new(&config);
        let sts = StsClient::new(&config);

        Ok(Self {
            ecr,
            sts,
            region: region.to_string(),
            retry_config: RetryConfig::default(),
        })
    }

    pub async fn get_caller_identity(&self) -> Result<(String, String)> {
        let _ecr = self.ecr.clone();
        let sts = self.sts.clone();
        let retry_config = self.retry_config.clone();
        
        let result = with_retry(&retry_config, "AWS get_caller_identity", || {
            let sts = sts.clone();
            async move {
                let response = sts
                    .get_caller_identity()
                    .send()
                    .await
                    .map_err(|e| AwsRetryableError(AwsError::SdkError(e.to_string())))?;

                let account = response
                    .account()
                    .ok_or_else(|| AwsRetryableError(AwsError::CredentialsInvalid))?
                    .to_string();

                let arn = response
                    .arn()
                    .ok_or_else(|| AwsRetryableError(AwsError::CredentialsInvalid))?
                    .to_string();

                Ok::<(String, String), AwsRetryableError>((account, arn))
            }
        }).await?;
        
        Ok(result)
    }

    pub async fn get_ecr_authorization(&self) -> Result<DockerCredentials> {
        let ecr = self.ecr.clone();
        let retry_config = self.retry_config.clone();
        
        let result = with_retry(&retry_config, "AWS get_ecr_authorization", || {
            let ecr = ecr.clone();
            async move {
                let response = ecr
                    .get_authorization_token()
                    .send()
                    .await
                    .map_err(|e| AwsRetryableError(AwsError::EcrAuthFailed(e.to_string())))?;

                let auth_data = response
                    .authorization_data()
                    .first()
                    .ok_or_else(|| AwsRetryableError(AwsError::EcrAuthFailed("No authorization data returned".to_string())))?;

                let token = auth_data
                    .authorization_token()
                    .ok_or_else(|| AwsRetryableError(AwsError::EcrAuthFailed("No token in response".to_string())))?;

                // Token is base64 encoded "username:password"
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(token)
                    .map_err(|e| AwsRetryableError(AwsError::EcrAuthFailed(format!("Failed to decode token: {}", e))))?;

                let decoded_str = String::from_utf8(decoded)
                    .map_err(|e| AwsRetryableError(AwsError::EcrAuthFailed(format!("Invalid token encoding: {}", e))))?;

                let (username, password) = decoded_str
                    .split_once(':')
                    .ok_or_else(|| AwsRetryableError(AwsError::EcrAuthFailed("Invalid token format".to_string())))?;

                let server_address = auth_data.proxy_endpoint().map(|s| s.to_string());

                Ok::<DockerCredentials, AwsRetryableError>(DockerCredentials {
                    username: Some(username.to_string()),
                    password: Some(password.to_string()),
                    serveraddress: server_address,
                    ..Default::default()
                })
            }
        }).await?;
        
        Ok(result)
    }

    pub fn get_ecr_registry_url(&self, account_id: &str) -> String {
        format!("{}.dkr.ecr.{}.amazonaws.com", account_id, self.region)
    }

    /// Helper to check if an ECR error indicates the repository was not found
    fn is_repository_not_found_error<E>(error: &SdkError<E>) -> bool 
    where
        E: std::fmt::Debug,
    {
        // Check the error code from the service error if available
        match error {
            SdkError::ServiceError(service_err) => {
                // The error message/code typically contains "RepositoryNotFoundException"
                let debug_str = format!("{:?}", service_err);
                debug_str.contains("RepositoryNotFoundException")
                    || debug_str.contains("RepositoryNotFound")
            }
            _ => {
                // For other SDK errors, check the string representation
                let err_str = error.to_string();
                err_str.contains("RepositoryNotFoundException")
                    || err_str.contains("RepositoryNotFound")
            }
        }
    }

    pub async fn ensure_repository_exists(&self, repo_name: &str) -> Result<String> {
        let ecr = self.ecr.clone();
        let repo_name = repo_name.to_string();
        let retry_config = self.retry_config.clone();
        
        let result = with_retry(&retry_config, "AWS ensure_repository_exists", || {
            let ecr = ecr.clone();
            let repo_name = repo_name.clone();
            async move {
                match ecr
                    .describe_repositories()
                    .repository_names(&repo_name)
                    .send()
                    .await
                {
                    Ok(response) => {
                        let repo = response
                            .repositories()
                            .first()
                            .ok_or_else(|| AwsRetryableError(AwsError::EcrRepoNotFound(repo_name.clone())))?;

                        repo.repository_uri()
                            .map(|s| s.to_string())
                            .ok_or_else(|| AwsRetryableError(AwsError::SdkError(format!(
                                "Repository '{}' exists but has no URI", repo_name
                            ))))
                    }
                    Err(e) => {
                        // Use proper error type checking instead of string matching
                        if Self::is_repository_not_found_error(&e) {
                            Err(AwsRetryableError(AwsError::EcrRepoNotFound(repo_name.clone())))
                        } else {
                            // Check if it's a retryable error
                            Err(AwsRetryableError(AwsError::SdkError(e.to_string())))
                        }
                    }
                }
            }
        }).await?;
        
        Ok(result)
    }

    pub async fn create_repository(&self, repo_name: &str) -> Result<String> {
        let ecr = self.ecr.clone();
        let repo_name = repo_name.to_string();
        let retry_config = self.retry_config.clone();
        
        let result = with_retry(&retry_config, "AWS create_repository", || {
            let ecr = ecr.clone();
            let repo_name = repo_name.clone();
            async move {
                let response = ecr
                    .create_repository()
                    .repository_name(&repo_name)
                    .image_scanning_configuration(
                        aws_sdk_ecr::types::ImageScanningConfiguration::builder()
                            .scan_on_push(true)
                            .build(),
                    )
                    .send()
                    .await
                    .map_err(|e| AwsRetryableError(AwsError::SdkError(e.to_string())))?;

                let repo = response
                    .repository()
                    .ok_or_else(|| AwsRetryableError(AwsError::SdkError("No repository in response".to_string())))?;

                repo.repository_uri()
                    .map(|s| s.to_string())
                    .ok_or_else(|| AwsRetryableError(AwsError::SdkError(format!(
                        "Created repository '{}' but it has no URI", repo_name
                    ))))
            }
        }).await?;
        
        Ok(result)
    }

    pub async fn list_image_tags(&self, repo_name: &str) -> Result<Vec<String>> {
        let ecr = self.ecr.clone();
        let repo_name = repo_name.to_string();
        let retry_config = self.retry_config.clone();
        
        let result = with_retry(&retry_config, "AWS list_image_tags", || {
            let ecr = ecr.clone();
            let repo_name = repo_name.clone();
            async move {
                let mut tags = Vec::new();
                let mut next_token: Option<String> = None;

                loop {
                    let mut request = ecr.list_images().repository_name(&repo_name);

                    if let Some(token) = &next_token {
                        request = request.next_token(token);
                    }

                    let response = request
                        .send()
                        .await
                        .map_err(|e| AwsRetryableError(AwsError::SdkError(e.to_string())))?;

                    for image_id in response.image_ids() {
                        if let Some(tag) = image_id.image_tag() {
                            tags.push(tag.to_string());
                        }
                    }

                    match response.next_token() {
                        Some(token) => next_token = Some(token.to_string()),
                        None => break,
                    }
                }

                Ok::<Vec<String>, AwsRetryableError>(tags)
            }
        }).await?;
        
        Ok(result)
    }
}
