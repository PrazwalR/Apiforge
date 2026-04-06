use crate::error::{GitHubError, Result};
use crate::utils::{RetryConfig, RetryableError, with_retry};
use octocrab::models::repos::Release;
use octocrab::Octocrab;
use std::sync::Arc;

/// Wrapper for GitHub errors that implements RetryableError
#[derive(Debug)]
struct GitHubRetryableError(GitHubError);

impl std::fmt::Display for GitHubRetryableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl RetryableError for GitHubRetryableError {
    fn is_retryable(&self) -> bool {
        match &self.0 {
            // API errors may be transient (rate limits, server errors)
            GitHubError::ApiError(msg) => {
                msg.contains("rate limit")
                    || msg.contains("429")
                    || msg.contains("500")
                    || msg.contains("502")
                    || msg.contains("503")
                    || msg.contains("504")
                    || msg.contains("timeout")
                    || msg.contains("connection")
            }
            // Auth and config errors are permanent
            GitHubError::TokenInvalid => false,
            GitHubError::RepoNotFound(_) => false,
            GitHubError::ReleaseFailed(msg) => {
                // Retry on transient errors during release creation
                msg.contains("rate limit")
                    || msg.contains("500")
                    || msg.contains("502")
                    || msg.contains("503")
                    || msg.contains("timeout")
            }
            GitHubError::PermissionDenied(_) => false,
        }
    }
}

impl From<GitHubRetryableError> for crate::error::ApiForgError {
    fn from(e: GitHubRetryableError) -> Self {
        crate::error::ApiForgError::GitHub(e.0)
    }
}

pub struct GitHubClient {
    octocrab: Arc<Octocrab>,
    owner: String,
    repo: String,
    retry_config: RetryConfig,
}

#[derive(Debug, Clone)]
pub struct ReleaseConfig {
    pub tag_name: String,
    pub name: String,
    pub body: String,
    pub draft: bool,
    pub prerelease: bool,
}

impl GitHubClient {
    pub async fn new(token: &str, repository: &str) -> Result<Self> {
        let octocrab = Arc::new(Octocrab::builder()
            .personal_token(token.to_string())
            .build()
            .map_err(|_e| GitHubError::TokenInvalid)?);

        let (owner, repo) = parse_repository(repository)?;
        let retry_config = RetryConfig::default();

        // Verify access by getting repo info with retry
        let octocrab_clone = octocrab.clone();
        let owner_clone = owner.clone();
        let repo_clone = repo.clone();
        let repository = repository.to_string();
        
        with_retry(&retry_config, "GitHub verify repository access", || {
            let octocrab = octocrab_clone.clone();
            let owner = owner_clone.clone();
            let repo = repo_clone.clone();
            let repository = repository.clone();
            async move {
                octocrab
                    .repos(&owner, &repo)
                    .get()
                    .await
                    .map_err(|e| {
                        let err = if e.to_string().contains("404") {
                            GitHubError::RepoNotFound(repository)
                        } else if e.to_string().contains("401") || e.to_string().contains("403") {
                            GitHubError::TokenInvalid
                        } else {
                            GitHubError::ApiError(e.to_string())
                        };
                        GitHubRetryableError(err)
                    })?;
                Ok::<(), GitHubRetryableError>(())
            }
        }).await?;

        Ok(Self {
            octocrab,
            owner,
            repo,
            retry_config,
        })
    }

    pub async fn create_release(&self, config: &ReleaseConfig) -> Result<Release> {
        let octocrab = self.octocrab.clone();
        let owner = self.owner.clone();
        let repo = self.repo.clone();
        let config = config.clone();
        let retry_config = self.retry_config.clone();
        
        let result = with_retry(&retry_config, "GitHub create release", || {
            let octocrab = octocrab.clone();
            let owner = owner.clone();
            let repo = repo.clone();
            let config = config.clone();
            async move {
                let release = octocrab
                    .repos(&owner, &repo)
                    .releases()
                    .create(&config.tag_name)
                    .name(&config.name)
                    .body(&config.body)
                    .draft(config.draft)
                    .prerelease(config.prerelease)
                    .send()
                    .await
                    .map_err(|e| GitHubRetryableError(GitHubError::ReleaseFailed(e.to_string())))?;

                Ok::<Release, GitHubRetryableError>(release)
            }
        }).await?;

        Ok(result)
    }

    pub async fn get_release_by_tag(&self, tag: &str) -> Result<Option<Release>> {
        let octocrab = self.octocrab.clone();
        let owner = self.owner.clone();
        let repo = self.repo.clone();
        let tag = tag.to_string();
        let retry_config = self.retry_config.clone();
        
        let result = with_retry(&retry_config, "GitHub get release by tag", || {
            let octocrab = octocrab.clone();
            let owner = owner.clone();
            let repo = repo.clone();
            let tag = tag.clone();
            async move {
                match octocrab
                    .repos(&owner, &repo)
                    .releases()
                    .get_by_tag(&tag)
                    .await
                {
                    Ok(release) => Ok::<Option<Release>, GitHubRetryableError>(Some(release)),
                    Err(e) if e.to_string().contains("404") => Ok(None),
                    Err(e) => Err(GitHubRetryableError(GitHubError::ApiError(e.to_string()))),
                }
            }
        }).await?;

        Ok(result)
    }

    pub async fn get_latest_release(&self) -> Result<Option<Release>> {
        let octocrab = self.octocrab.clone();
        let owner = self.owner.clone();
        let repo = self.repo.clone();
        let retry_config = self.retry_config.clone();
        
        let result = with_retry(&retry_config, "GitHub get latest release", || {
            let octocrab = octocrab.clone();
            let owner = owner.clone();
            let repo = repo.clone();
            async move {
                match octocrab
                    .repos(&owner, &repo)
                    .releases()
                    .get_latest()
                    .await
                {
                    Ok(release) => Ok::<Option<Release>, GitHubRetryableError>(Some(release)),
                    Err(e) if e.to_string().contains("404") => Ok(None),
                    Err(e) => Err(GitHubRetryableError(GitHubError::ApiError(e.to_string()))),
                }
            }
        }).await?;

        Ok(result)
    }

    pub async fn list_releases(&self, per_page: u8) -> Result<Vec<Release>> {
        let octocrab = self.octocrab.clone();
        let owner = self.owner.clone();
        let repo = self.repo.clone();
        let retry_config = self.retry_config.clone();
        
        let result = with_retry(&retry_config, "GitHub list releases", || {
            let octocrab = octocrab.clone();
            let owner = owner.clone();
            let repo = repo.clone();
            async move {
                let releases = octocrab
                    .repos(&owner, &repo)
                    .releases()
                    .list()
                    .per_page(per_page)
                    .send()
                    .await
                    .map_err(|e| GitHubRetryableError(GitHubError::ApiError(e.to_string())))?;

                Ok::<Vec<Release>, GitHubRetryableError>(releases.items)
            }
        }).await?;

        Ok(result)
    }

    pub async fn update_release(
        &self,
        release_id: u64,
        body: Option<&str>,
        draft: Option<bool>,
        prerelease: Option<bool>,
    ) -> Result<Release> {
        let octocrab = self.octocrab.clone();
        let owner = self.owner.clone();
        let repo = self.repo.clone();
        let retry_config = self.retry_config.clone();
        
        // Build request body outside the closure
        let mut request_body = serde_json::Map::new();
        if let Some(b) = body {
            request_body.insert("body".to_string(), serde_json::Value::String(b.to_string()));
        }
        if let Some(d) = draft {
            request_body.insert("draft".to_string(), serde_json::Value::Bool(d));
        }
        if let Some(p) = prerelease {
            request_body.insert("prerelease".to_string(), serde_json::Value::Bool(p));
        }
        let request_body = serde_json::Value::Object(request_body);
        
        let result = with_retry(&retry_config, "GitHub update release", || {
            let octocrab = octocrab.clone();
            let owner = owner.clone();
            let repo = repo.clone();
            let request_body = request_body.clone();
            async move {
                let url = format!("repos/{}/{}/releases/{}", owner, repo, release_id);

                let release: Release = octocrab
                    .patch(url, Some(&request_body))
                    .await
                    .map_err(|e| GitHubRetryableError(GitHubError::ApiError(e.to_string())))?;

                Ok::<Release, GitHubRetryableError>(release)
            }
        }).await?;

        Ok(result)
    }

    pub async fn delete_release(&self, release_id: u64) -> Result<()> {
        let octocrab = self.octocrab.clone();
        let owner = self.owner.clone();
        let repo = self.repo.clone();
        let retry_config = self.retry_config.clone();
        
        with_retry(&retry_config, "GitHub delete release", || {
            let octocrab = octocrab.clone();
            let owner = owner.clone();
            let repo = repo.clone();
            async move {
                octocrab
                    .repos(&owner, &repo)
                    .releases()
                    .delete(release_id)
                    .await
                    .map_err(|e| GitHubRetryableError(GitHubError::ApiError(e.to_string())))?;

                Ok::<(), GitHubRetryableError>(())
            }
        }).await?;

        Ok(())
    }

    pub async fn generate_release_notes(
        &self,
        tag_name: &str,
        previous_tag: Option<&str>,
    ) -> Result<String> {
        let octocrab = self.octocrab.clone();
        let owner = self.owner.clone();
        let repo = self.repo.clone();
        let tag_name = tag_name.to_string();
        let previous_tag = previous_tag.map(|s| s.to_string());
        let retry_config = self.retry_config.clone();
        
        let result = with_retry(&retry_config, "GitHub generate release notes", || {
            let octocrab = octocrab.clone();
            let owner = owner.clone();
            let repo = repo.clone();
            let tag_name = tag_name.clone();
            let previous_tag = previous_tag.clone();
            async move {
                let url = format!("repos/{}/{}/releases/generate-notes", owner, repo);

                let mut body = serde_json::json!({
                    "tag_name": tag_name
                });

                if let Some(prev) = previous_tag {
                    body["previous_tag_name"] = serde_json::Value::String(prev);
                }

                let response: serde_json::Value = octocrab
                    .post(url, Some(&body))
                    .await
                    .map_err(|e| GitHubRetryableError(GitHubError::ApiError(e.to_string())))?;

                response
                    .get("body")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .ok_or_else(|| GitHubRetryableError(GitHubError::ApiError("No body in response".to_string())))
            }
        }).await?;

        Ok(result)
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn repo(&self) -> &str {
        &self.repo
    }
}

fn parse_repository(repo: &str) -> Result<(String, String)> {
    // Handle formats: "owner/repo" or "https://github.com/owner/repo"
    let repo = repo
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .trim_start_matches("https://github.com/")
        .trim_start_matches("git@github.com:");

    let parts: Vec<&str> = repo.split('/').collect();
    if parts.len() != 2 {
        return Err(GitHubError::ApiError(format!(
            "Invalid repository format: {}. Expected 'owner/repo'",
            repo
        ))
        .into());
    }

    Ok((parts[0].to_string(), parts[1].to_string()))
}
