use crate::error::{GitError, Result};
use git2::{BranchType, Cred, FetchOptions, ObjectType, PushOptions, RemoteCallbacks, Repository};
use std::path::{Path, PathBuf};

pub struct GitRepo {
    repo: Repository,
}

pub struct CommitInfo {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub timestamp: i64,
}

fn build_remote_callbacks<'repo>(repo: &'repo Repository) -> RemoteCallbacks<'repo> {
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(move |url, username_from_url, allowed_types| {
        // Prefer SSH agent credentials when available.
        if allowed_types.is_ssh_key() {
            if let Some(username) = username_from_url {
                if let Ok(cred) = Cred::ssh_key_from_agent(username) {
                    return Ok(cred);
                }
            }
        }

        // Fallback to git credential helpers (supports HTTPS/token flows on Windows/macOS/Linux).
        if allowed_types.is_user_pass_plaintext() {
            if let Ok(config) = repo.config() {
                if let Ok(cred) = Cred::credential_helper(&config, url, username_from_url) {
                    return Ok(cred);
                }
            }
        }

        // Username-only credential prompt fallback.
        if allowed_types.is_username() {
            if let Some(username) = username_from_url {
                if let Ok(cred) = Cred::username(username) {
                    return Ok(cred);
                }
            }
        }

        Cred::default()
    });
    callbacks
}

impl GitRepo {
    pub fn open() -> Result<Self> {
        let repo = Repository::open_from_env()
            .or_else(|_| Repository::discover("."))
            .map_err(|_| GitError::NotARepository)?;

        Ok(Self { repo })
    }

    pub fn open_at(path: &Path) -> Result<Self> {
        let repo = Repository::open(path).map_err(|_| GitError::NotARepository)?;
        Ok(Self { repo })
    }

    pub fn current_branch(&self) -> Result<String> {
        let head = self
            .repo
            .head()
            .map_err(|e| GitError::GitOperation(format!("Failed to get HEAD: {}", e)))?;

        let branch_name = head
            .shorthand()
            .ok_or_else(|| GitError::GitOperation("Failed to get branch name".to_string()))?;

        Ok(branch_name.to_string())
    }

    pub fn is_working_tree_clean(&self) -> Result<bool> {
        let statuses = self
            .repo
            .statuses(None)
            .map_err(|e| GitError::GitOperation(format!("Failed to get status: {}", e)))?;

        Ok(statuses.is_empty())
    }

    pub fn get_uncommitted_changes(&self) -> Result<Vec<String>> {
        let statuses = self
            .repo
            .statuses(None)
            .map_err(|e| GitError::GitOperation(format!("Failed to get status: {}", e)))?;

        let mut changes = Vec::new();
        for entry in statuses.iter() {
            if let Some(path) = entry.path() {
                changes.push(path.to_string());
            }
        }

        Ok(changes)
    }

    pub fn fetch(&self, remote_name: &str) -> Result<()> {
        let mut remote = self
            .repo
            .find_remote(remote_name)
            .map_err(|_| GitError::RemoteNotFound(remote_name.to_string()))?;

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(build_remote_callbacks(&self.repo));

        remote
            .fetch(&[] as &[&str], Some(&mut fetch_options), None)
            .map_err(|e| GitError::GitOperation(format!("Failed to fetch from remote: {}", e)))?;

        Ok(())
    }

    pub fn check_remote_sync(&self, branch: &str, remote: &str) -> Result<(usize, usize)> {
        let local_branch = self
            .repo
            .find_branch(branch, BranchType::Local)
            .map_err(|e| GitError::GitOperation(format!("Failed to find local branch: {}", e)))?;

        let local_oid = local_branch.get().target().ok_or_else(|| {
            GitError::GitOperation("Failed to get local branch target".to_string())
        })?;

        let remote_branch_name = format!("{}/{}", remote, branch);
        let remote_branch = match self
            .repo
            .find_branch(&remote_branch_name, BranchType::Remote)
        {
            Ok(b) => b,
            Err(_) => return Ok((0, 0)),
        };

        let remote_oid = remote_branch.get().target().ok_or_else(|| {
            GitError::GitOperation("Failed to get remote branch target".to_string())
        })?;

        let (ahead, behind) = self
            .repo
            .graph_ahead_behind(local_oid, remote_oid)
            .map_err(|e| GitError::GitOperation(format!("Failed to compare branches: {}", e)))?;

        Ok((ahead, behind))
    }

    pub fn get_latest_tag(&self, pattern: &str) -> Result<Option<String>> {
        let tags = self
            .repo
            .tag_names(Some(pattern))
            .map_err(|e| GitError::GitOperation(format!("Failed to get tags: {}", e)))?;

        let mut tag_list: Vec<String> = tags.iter().filter_map(|t| t.map(String::from)).collect();

        tag_list.sort_by(|a, b| {
            let a_ver = semver::Version::parse(a.trim_start_matches('v'));
            let b_ver = semver::Version::parse(b.trim_start_matches('v'));

            match (a_ver, b_ver) {
                (Ok(av), Ok(bv)) => bv.cmp(&av),
                _ => b.cmp(a),
            }
        });

        Ok(tag_list.first().cloned())
    }

    pub fn get_commits_between(&self, from: &str, to: &str) -> Result<Vec<CommitInfo>> {
        let from_obj = self
            .repo
            .revparse_single(from)
            .map_err(|e| GitError::GitOperation(format!("Failed to parse from ref: {}", e)))?;

        let to_obj = self
            .repo
            .revparse_single(to)
            .map_err(|e| GitError::GitOperation(format!("Failed to parse to ref: {}", e)))?;

        let mut revwalk = self
            .repo
            .revwalk()
            .map_err(|e| GitError::GitOperation(format!("Failed to create revwalk: {}", e)))?;

        revwalk
            .push(to_obj.id())
            .map_err(|e| GitError::GitOperation(format!("Failed to push to revwalk: {}", e)))?;

        revwalk
            .hide(from_obj.id())
            .map_err(|e| GitError::GitOperation(format!("Failed to hide from revwalk: {}", e)))?;

        let mut commits = Vec::new();
        for oid in revwalk {
            let oid = oid
                .map_err(|e| GitError::GitOperation(format!("Failed to get commit oid: {}", e)))?;
            let commit = self
                .repo
                .find_commit(oid)
                .map_err(|e| GitError::GitOperation(format!("Failed to find commit: {}", e)))?;

            commits.push(CommitInfo {
                sha: commit.id().to_string(),
                message: commit.message().unwrap_or("").to_string(),
                author: commit.author().name().unwrap_or("").to_string(),
                timestamp: commit.time().seconds(),
            });
        }

        Ok(commits)
    }

    pub fn commit(&self, message: &str) -> Result<String> {
        let signature = self
            .repo
            .signature()
            .map_err(|e| GitError::CommitFailed(format!("Failed to get signature: {}", e)))?;

        let mut index = self
            .repo
            .index()
            .map_err(|e| GitError::CommitFailed(format!("Failed to get index: {}", e)))?;

        let tree_oid = index
            .write_tree()
            .map_err(|e| GitError::CommitFailed(format!("Failed to write tree: {}", e)))?;

        let tree = self
            .repo
            .find_tree(tree_oid)
            .map_err(|e| GitError::CommitFailed(format!("Failed to find tree: {}", e)))?;

        let parent_commit = self
            .repo
            .head()
            .map_err(|e| GitError::CommitFailed(format!("Failed to get HEAD: {}", e)))?
            .peel_to_commit()
            .map_err(|e| GitError::CommitFailed(format!("Failed to peel to commit: {}", e)))?;

        let oid = self
            .repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &[&parent_commit],
            )
            .map_err(|e| GitError::CommitFailed(format!("Failed to create commit: {}", e)))?;

        Ok(oid.to_string())
    }

    pub fn add(&self, path: &Path) -> Result<()> {
        let mut index = self
            .repo
            .index()
            .map_err(|e| GitError::GitOperation(format!("Failed to get index: {}", e)))?;

        index
            .add_path(path)
            .map_err(|e| GitError::GitOperation(format!("Failed to add path: {}", e)))?;

        index
            .write()
            .map_err(|e| GitError::GitOperation(format!("Failed to write index: {}", e)))?;

        Ok(())
    }

    pub fn create_tag(&self, name: &str, message: &str) -> Result<()> {
        let obj = self
            .repo
            .head()
            .map_err(|e| GitError::TagFailed(format!("Failed to get HEAD: {}", e)))?
            .peel(ObjectType::Commit)
            .map_err(|e| GitError::TagFailed(format!("Failed to peel to commit: {}", e)))?;

        let signature = self
            .repo
            .signature()
            .map_err(|e| GitError::TagFailed(format!("Failed to get signature: {}", e)))?;

        self.repo
            .tag(name, &obj, &signature, message, false)
            .map_err(|e| GitError::TagFailed(format!("Failed to create tag: {}", e)))?;

        Ok(())
    }

    pub fn push(&self, remote: &str, refspec: &str) -> Result<()> {
        let mut remote = self
            .repo
            .find_remote(remote)
            .map_err(|_| GitError::RemoteNotFound(remote.to_string()))?;

        let mut push_options = PushOptions::new();
        push_options.remote_callbacks(build_remote_callbacks(&self.repo));

        remote
            .push(&[refspec], Some(&mut push_options))
            .map_err(|e| GitError::PushFailed(format!("Failed to push: {}", e)))?;

        Ok(())
    }

    /// Push with timeout support - async version that respects GitTimeoutConfig
    pub async fn push_with_timeout(
        &self,
        remote: &str,
        refspec: &str,
        timeout_config: &super::GitTimeoutConfig,
    ) -> Result<()> {
        use super::push_with_timeout;

        let remote = remote.to_string();
        let refspec = refspec.to_string();

        // Clone the repository reference for use in the blocking task
        let repo_path = self.root_path();

        push_with_timeout(
            move || {
                let repo = Repository::open(&repo_path)
                    .map_err(|e| GitError::GitOperation(format!("Failed to open repo: {}", e)))?;
                let mut remote = repo
                    .find_remote(&remote)
                    .map_err(|_| GitError::RemoteNotFound(remote.clone()))?;

                let mut push_options = PushOptions::new();
                push_options.remote_callbacks(build_remote_callbacks(&repo));

                remote
                    .push(&[&refspec], Some(&mut push_options))
                    .map_err(|e| GitError::PushFailed(format!("Failed to push: {}", e)))?;

                Ok(())
            },
            timeout_config,
        )
        .await
    }

    pub fn current_commit_sha(&self) -> Result<String> {
        let head = self
            .repo
            .head()
            .map_err(|e| GitError::GitOperation(format!("Failed to get HEAD: {}", e)))?;

        let commit = head
            .peel_to_commit()
            .map_err(|e| GitError::GitOperation(format!("Failed to peel to commit: {}", e)))?;

        Ok(commit.id().to_string())
    }

    pub fn root_path(&self) -> PathBuf {
        self.repo
            .workdir()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    }

    pub fn remote_exists(&self, remote: &str) -> bool {
        self.repo.find_remote(remote).is_ok()
    }

    pub fn remote_url(&self, remote: &str) -> Result<String> {
        let remote = self
            .repo
            .find_remote(remote)
            .map_err(|_| GitError::RemoteNotFound(remote.to_string()))?;

        let url = remote
            .url()
            .ok_or_else(|| GitError::GitOperation("Remote URL not found".to_string()))?;

        Ok(url.to_string())
    }

    /// Check if a tag already exists
    pub fn tag_exists(&self, tag_name: &str) -> Result<bool> {
        match self.repo.find_reference(&format!("refs/tags/{}", tag_name)) {
            Ok(_) => Ok(true),
            Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(false),
            Err(e) => Err(GitError::GitOperation(format!("Failed to check tag: {}", e)).into()),
        }
    }

    /// Checkout (restore) a file from HEAD, discarding local changes
    pub fn checkout_file(&self, path: &Path) -> Result<()> {
        let head = self
            .repo
            .head()
            .map_err(|e| GitError::GitOperation(format!("Failed to get HEAD: {}", e)))?;

        let tree = head
            .peel_to_tree()
            .map_err(|e| GitError::GitOperation(format!("Failed to get tree: {}", e)))?;

        let mut checkout_builder = git2::build::CheckoutBuilder::new();
        checkout_builder.path(path);
        checkout_builder.force();

        self.repo
            .checkout_tree(tree.as_object(), Some(&mut checkout_builder))
            .map_err(|e| GitError::GitOperation(format!("Failed to checkout file: {}", e)))?;

        Ok(())
    }

    /// Delete a tag (for rollback)
    pub fn delete_tag(&self, tag_name: &str) -> Result<()> {
        let refname = format!("refs/tags/{}", tag_name);

        let mut reference = self
            .repo
            .find_reference(&refname)
            .map_err(|e| GitError::GitOperation(format!("Tag not found: {}", e)))?;

        reference
            .delete()
            .map_err(|e| GitError::GitOperation(format!("Failed to delete tag: {}", e)))?;

        Ok(())
    }

    /// Delete a remote tag by pushing an empty ref
    pub fn delete_remote_tag(&self, remote: &str, tag_name: &str) -> Result<()> {
        let mut remote = self
            .repo
            .find_remote(remote)
            .map_err(|_| GitError::RemoteNotFound(remote.to_string()))?;

        let mut push_options = PushOptions::new();
        push_options.remote_callbacks(build_remote_callbacks(&self.repo));

        let refspec = format!(":refs/tags/{}", tag_name);
        remote
            .push(&[refspec], Some(&mut push_options))
            .map_err(|e| GitError::PushFailed(format!("Failed to delete remote tag: {}", e)))?;

        Ok(())
    }

    /// Delete a remote tag with timeout support
    pub async fn delete_remote_tag_with_timeout(
        &self,
        remote: &str,
        tag_name: &str,
        timeout_config: &super::GitTimeoutConfig,
    ) -> Result<()> {
        use super::push_with_timeout;

        let remote = remote.to_string();
        let tag_name = tag_name.to_string();
        let repo_path = self.root_path();

        push_with_timeout(
            move || {
                let repo = Repository::open(&repo_path)
                    .map_err(|e| GitError::GitOperation(format!("Failed to open repo: {}", e)))?;
                let mut remote = repo
                    .find_remote(&remote)
                    .map_err(|_| GitError::RemoteNotFound(remote.clone()))?;

                let mut push_options = PushOptions::new();
                push_options.remote_callbacks(build_remote_callbacks(&repo));

                let refspec = format!(":refs/tags/{}", tag_name);
                remote
                    .push(&[refspec], Some(&mut push_options))
                    .map_err(|e| {
                        GitError::PushFailed(format!("Failed to delete remote tag: {}", e))
                    })?;

                Ok(())
            },
            timeout_config,
        )
        .await
    }

    /// Get the parent commit of HEAD (for rollback)
    pub fn get_parent_commit(&self) -> Result<Option<String>> {
        let head = self
            .repo
            .head()
            .map_err(|e| GitError::GitOperation(format!("Failed to get HEAD: {}", e)))?;

        let commit = head
            .peel_to_commit()
            .map_err(|e| GitError::GitOperation(format!("Failed to peel to commit: {}", e)))?;

        match commit.parent(0) {
            Ok(parent) => Ok(Some(parent.id().to_string())),
            Err(_) => Ok(None),
        }
    }

    /// Soft reset to a specific commit (keeps changes staged)
    pub fn reset_soft(&self, commit_sha: &str) -> Result<()> {
        let obj = self
            .repo
            .revparse_single(commit_sha)
            .map_err(|e| GitError::GitOperation(format!("Failed to parse commit: {}", e)))?;

        self.repo
            .reset(&obj, git2::ResetType::Soft, None)
            .map_err(|e| GitError::GitOperation(format!("Failed to soft reset: {}", e)))?;

        Ok(())
    }

    /// Create a revert commit for a given commit
    pub fn create_revert_commit(&self, commit_sha: &str, message: &str) -> Result<String> {
        let obj = self
            .repo
            .revparse_single(commit_sha)
            .map_err(|e| GitError::GitOperation(format!("Failed to find commit: {}", e)))?;

        let commit = obj
            .peel_to_commit()
            .map_err(|e| GitError::GitOperation(format!("Failed to peel to commit: {}", e)))?;

        let signature = self
            .repo
            .signature()
            .map_err(|e| GitError::CommitFailed(format!("Failed to get signature: {}", e)))?;

        let mut revert_options = git2::RevertOptions::new();
        self.repo
            .revert(&commit, Some(&mut revert_options))
            .map_err(|e| GitError::CommitFailed(format!("Failed to revert: {}", e)))?;

        let mut index = self
            .repo
            .index()
            .map_err(|e| GitError::GitOperation(format!("Failed to get index: {}", e)))?;
        let tree_oid = index
            .write_tree()
            .map_err(|e| GitError::GitOperation(format!("Failed to write tree: {}", e)))?;
        let tree = self
            .repo
            .find_tree(tree_oid)
            .map_err(|e| GitError::GitOperation(format!("Failed to find tree: {}", e)))?;

        let parent = self
            .repo
            .head()
            .map_err(|e| GitError::GitOperation(format!("Failed to get HEAD: {}", e)))?
            .peel_to_commit()
            .map_err(|e| GitError::GitOperation(format!("Failed to peel to commit: {}", e)))?;

        let oid = self
            .repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &[&parent],
            )
            .map_err(|e| GitError::CommitFailed(format!("Failed to create commit: {}", e)))?;

        self.repo
            .cleanup_state()
            .map_err(|e| GitError::GitOperation(format!("Failed to cleanup state: {}", e)))?;

        Ok(oid.to_string())
    }

    /// Get commit message for a given sha
    pub fn get_commit_message(&self, commit_sha: &str) -> Result<String> {
        let obj = self
            .repo
            .revparse_single(commit_sha)
            .map_err(|e| GitError::GitOperation(format!("Failed to find commit: {}", e)))?;

        let commit = obj
            .peel_to_commit()
            .map_err(|e| GitError::GitOperation(format!("Failed to peel to commit: {}", e)))?;

        Ok(commit.message().unwrap_or("").to_string())
    }
}
