use crate::error::{GitHubError, Result};
use octocrab::models::repos::Release;
use octocrab::Octocrab;

pub struct GitHubClient {
    octocrab: Octocrab,
    owner: String,
    repo: String,
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
        let octocrab = Octocrab::builder()
            .personal_token(token.to_string())
            .build()
            .map_err(|_e| GitHubError::TokenInvalid)?;

        let (owner, repo) = parse_repository(repository)?;

        // Verify access by getting repo info
        octocrab
            .repos(&owner, &repo)
            .get()
            .await
            .map_err(|e| {
                if e.to_string().contains("404") {
                    GitHubError::RepoNotFound(repository.to_string())
                } else if e.to_string().contains("401") || e.to_string().contains("403") {
                    GitHubError::TokenInvalid
                } else {
                    GitHubError::ApiError(e.to_string())
                }
            })?;

        Ok(Self {
            octocrab,
            owner,
            repo,
        })
    }

    pub async fn create_release(&self, config: &ReleaseConfig) -> Result<Release> {
        let release = self
            .octocrab
            .repos(&self.owner, &self.repo)
            .releases()
            .create(&config.tag_name)
            .name(&config.name)
            .body(&config.body)
            .draft(config.draft)
            .prerelease(config.prerelease)
            .send()
            .await
            .map_err(|e| GitHubError::ReleaseFailed(e.to_string()))?;

        Ok(release)
    }

    pub async fn get_release_by_tag(&self, tag: &str) -> Result<Option<Release>> {
        match self
            .octocrab
            .repos(&self.owner, &self.repo)
            .releases()
            .get_by_tag(tag)
            .await
        {
            Ok(release) => Ok(Some(release)),
            Err(e) if e.to_string().contains("404") => Ok(None),
            Err(e) => Err(GitHubError::ApiError(e.to_string()).into()),
        }
    }

    pub async fn get_latest_release(&self) -> Result<Option<Release>> {
        match self
            .octocrab
            .repos(&self.owner, &self.repo)
            .releases()
            .get_latest()
            .await
        {
            Ok(release) => Ok(Some(release)),
            Err(e) if e.to_string().contains("404") => Ok(None),
            Err(e) => Err(GitHubError::ApiError(e.to_string()).into()),
        }
    }

    pub async fn list_releases(&self, per_page: u8) -> Result<Vec<Release>> {
        let releases = self
            .octocrab
            .repos(&self.owner, &self.repo)
            .releases()
            .list()
            .per_page(per_page)
            .send()
            .await
            .map_err(|e| GitHubError::ApiError(e.to_string()))?;

        Ok(releases.items)
    }

    pub async fn update_release(
        &self,
        release_id: u64,
        body: Option<&str>,
        draft: Option<bool>,
        prerelease: Option<bool>,
    ) -> Result<Release> {
        // Note: This is a simplified version. For full support, we'd need to 
        // construct the API call differently since the builder pattern has lifetime issues.
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

        let url = format!(
            "repos/{}/{}/releases/{}",
            self.owner, self.repo, release_id
        );

        let release: Release = self
            .octocrab
            .patch(url, Some(&serde_json::Value::Object(request_body)))
            .await
            .map_err(|e| GitHubError::ApiError(e.to_string()))?;

        Ok(release)
    }

    pub async fn delete_release(&self, release_id: u64) -> Result<()> {
        self.octocrab
            .repos(&self.owner, &self.repo)
            .releases()
            .delete(release_id)
            .await
            .map_err(|e| GitHubError::ApiError(e.to_string()))?;

        Ok(())
    }

    pub async fn generate_release_notes(
        &self,
        tag_name: &str,
        previous_tag: Option<&str>,
    ) -> Result<String> {
        // GitHub API for generating release notes
        let url = format!(
            "repos/{}/{}/releases/generate-notes",
            self.owner, self.repo
        );

        let mut body = serde_json::json!({
            "tag_name": tag_name
        });

        if let Some(prev) = previous_tag {
            body["previous_tag_name"] = serde_json::Value::String(prev.to_string());
        }

        let response: serde_json::Value = self
            .octocrab
            .post(url, Some(&body))
            .await
            .map_err(|e| GitHubError::ApiError(e.to_string()))?;

        response
            .get("body")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| GitHubError::ApiError("No body in response".to_string()).into())
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
