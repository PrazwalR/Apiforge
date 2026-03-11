use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiForgError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Git error: {0}")]
    Git(#[from] GitError),

    #[error("Docker error: {0}")]
    Docker(#[from] DockerError),

    #[error("Kubernetes error: {0}")]
    Kubernetes(#[from] K8sError),

    #[error("AWS error: {0}")]
    Aws(#[from] AwsError),

    #[error("GitHub error: {0}")]
    GitHub(#[from] GitHubError),

    #[error("Preflight check failed: {0}")]
    PreflightFailed(String),

    #[error("Step execution failed: {0}")]
    StepFailed(String),

    #[error("Environment variable not set: {0}")]
    EnvVarMissing(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Invalid version: {0}")]
    InvalidVersion(String),

    #[error("Audit error: {0}")]
    Audit(String),
}

#[derive(Error, Debug)]
pub enum GitError {
    #[error("Repository not found or not a git repository")]
    NotARepository,

    #[error("Working directory is not clean: {0}")]
    DirtyWorkingTree(String),

    #[error("Not on required branch. Current: {current}, Required: {required}")]
    WrongBranch { current: String, required: String },

    #[error("Local branch is ahead of remote by {0} commits. Please push first.")]
    AheadOfRemote(usize),

    #[error("Local branch is behind remote by {0} commits. Please pull first.")]
    BehindRemote(usize),

    #[error("Remote '{0}' not found")]
    RemoteNotFound(String),

    #[error("Failed to create commit: {0}")]
    CommitFailed(String),

    #[error("Failed to create tag: {0}")]
    TagFailed(String),

    #[error("Failed to push: {0}")]
    PushFailed(String),

    #[error("Git operation failed: {0}")]
    GitOperation(String),

    #[error("libgit2 error: {0}")]
    Git2(#[from] git2::Error),
}

#[derive(Error, Debug)]
pub enum DockerError {
    #[error("Docker daemon not running or not accessible")]
    DaemonNotAccessible,

    #[error("Build failed: {0}")]
    BuildFailed(String),

    #[error("Tag failed: {0}")]
    TagFailed(String),

    #[error("Push failed: {0}")]
    PushFailed(String),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Image not found: {0}")]
    ImageNotFound(String),

    #[error("Bollard error: {0}")]
    Bollard(String),
}

#[derive(Error, Debug)]
pub enum K8sError {
    #[error("Kubeconfig not found or invalid")]
    KubeconfigInvalid,

    #[error("Context '{0}' not found in kubeconfig")]
    ContextNotFound(String),

    #[error("Cluster not reachable: {0}")]
    ClusterUnreachable(String),

    #[error("Namespace '{0}' not found")]
    NamespaceNotFound(String),

    #[error("Deployment '{0}' not found in namespace '{1}'")]
    DeploymentNotFound(String, String),

    #[error("Deployment rollout failed: {0}")]
    RolloutFailed(String),

    #[error("Rollout timeout after {0} seconds")]
    RolloutTimeout(u64),

    #[error("Manifest error: {0}")]
    ManifestError(String),

    #[error("Insufficient permissions: {0}")]
    PermissionDenied(String),

    #[error("Kube API error: {0}")]
    KubeApi(String),
}

#[derive(Error, Debug)]
pub enum AwsError {
    #[error("AWS credentials not found or invalid")]
    CredentialsInvalid,

    #[error("ECR repository not found: {0}")]
    EcrRepoNotFound(String),

    #[error("ECR authentication failed: {0}")]
    EcrAuthFailed(String),

    #[error("Insufficient IAM permissions: {0}")]
    PermissionDenied(String),

    #[error("AWS region not configured")]
    RegionNotConfigured,

    #[error("AWS SDK error: {0}")]
    SdkError(String),
}

#[derive(Error, Debug)]
pub enum GitHubError {
    #[error("GitHub token not set or invalid")]
    TokenInvalid,

    #[error("Repository not found: {0}")]
    RepoNotFound(String),

    #[error("Release creation failed: {0}")]
    ReleaseFailed(String),

    #[error("Insufficient permissions: {0}")]
    PermissionDenied(String),

    #[error("GitHub API error: {0}")]
    ApiError(String),
}

pub type Result<T> = std::result::Result<T, ApiForgError>;
