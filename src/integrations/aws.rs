use crate::error::{AwsError, Result};
use aws_config::BehaviorVersion;
use aws_sdk_ecr::Client as EcrClient;
use aws_sdk_sts::Client as StsClient;
use base64::Engine;
use bollard::auth::DockerCredentials;

pub struct AwsClient {
    ecr: EcrClient,
    sts: StsClient,
    region: String,
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
        })
    }

    pub async fn get_caller_identity(&self) -> Result<(String, String)> {
        let response = self
            .sts
            .get_caller_identity()
            .send()
            .await
            .map_err(|e| AwsError::SdkError(e.to_string()))?;

        let account = response
            .account()
            .ok_or(AwsError::CredentialsInvalid)?
            .to_string();

        let arn = response
            .arn()
            .ok_or(AwsError::CredentialsInvalid)?
            .to_string();

        Ok((account, arn))
    }

    pub async fn get_ecr_authorization(&self) -> Result<DockerCredentials> {
        let response = self
            .ecr
            .get_authorization_token()
            .send()
            .await
            .map_err(|e| AwsError::EcrAuthFailed(e.to_string()))?;

        let auth_data = response
            .authorization_data()
            .first()
            .ok_or_else(|| AwsError::EcrAuthFailed("No authorization data returned".to_string()))?;

        let token = auth_data
            .authorization_token()
            .ok_or_else(|| AwsError::EcrAuthFailed("No token in response".to_string()))?;

        // Token is base64 encoded "username:password"
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(token)
            .map_err(|e| AwsError::EcrAuthFailed(format!("Failed to decode token: {}", e)))?;

        let decoded_str = String::from_utf8(decoded)
            .map_err(|e| AwsError::EcrAuthFailed(format!("Invalid token encoding: {}", e)))?;

        let (username, password) = decoded_str
            .split_once(':')
            .ok_or_else(|| AwsError::EcrAuthFailed("Invalid token format".to_string()))?;

        let server_address = auth_data.proxy_endpoint().map(|s| s.to_string());

        Ok(DockerCredentials {
            username: Some(username.to_string()),
            password: Some(password.to_string()),
            serveraddress: server_address,
            ..Default::default()
        })
    }

    pub fn get_ecr_registry_url(&self, account_id: &str) -> String {
        format!("{}.dkr.ecr.{}.amazonaws.com", account_id, self.region)
    }

    pub async fn ensure_repository_exists(&self, repo_name: &str) -> Result<String> {
        // Try to describe the repository first
        match self
            .ecr
            .describe_repositories()
            .repository_names(repo_name)
            .send()
            .await
        {
            Ok(response) => {
                let repo = response
                    .repositories()
                    .first()
                    .ok_or_else(|| AwsError::EcrRepoNotFound(repo_name.to_string()))?;

                repo.repository_uri()
                    .map(|s| s.to_string())
                    .ok_or_else(|| AwsError::SdkError(format!(
                        "Repository '{}' exists but has no URI", repo_name
                    )).into())
            }
            Err(e) => {
                // Check if it's a RepositoryNotFoundException
                let err_str = e.to_string();
                if err_str.contains("RepositoryNotFoundException") {
                    Err(AwsError::EcrRepoNotFound(repo_name.to_string()).into())
                } else {
                    Err(AwsError::SdkError(err_str).into())
                }
            }
        }
    }

    pub async fn create_repository(&self, repo_name: &str) -> Result<String> {
        let response = self
            .ecr
            .create_repository()
            .repository_name(repo_name)
            .image_scanning_configuration(
                aws_sdk_ecr::types::ImageScanningConfiguration::builder()
                    .scan_on_push(true)
                    .build(),
            )
            .send()
            .await
            .map_err(|e| AwsError::SdkError(e.to_string()))?;

        let repo = response
            .repository()
            .ok_or_else(|| AwsError::SdkError("No repository in response".to_string()))?;

        repo.repository_uri()
            .map(|s| s.to_string())
            .ok_or_else(|| AwsError::SdkError(format!(
                "Created repository '{}' but it has no URI", repo_name
            )).into())
    }

    pub async fn list_image_tags(&self, repo_name: &str) -> Result<Vec<String>> {
        let mut tags = Vec::new();
        let mut next_token: Option<String> = None;

        loop {
            let mut request = self.ecr.list_images().repository_name(repo_name);

            if let Some(token) = next_token {
                request = request.next_token(token);
            }

            let response = request
                .send()
                .await
                .map_err(|e| AwsError::SdkError(e.to_string()))?;

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

        Ok(tags)
    }
}
