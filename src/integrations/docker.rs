use crate::error::{DockerError, Result};
use bollard::auth::DockerCredentials;
use bollard::image::{BuildImageOptions, PushImageOptions, TagImageOptions};
use bollard::Docker;
use futures::StreamExt;
use std::collections::HashMap;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

pub struct DockerClient {
    docker: Docker,
}

#[derive(Debug, Clone)]
pub struct BuildConfig {
    pub dockerfile: String,
    pub context: String,
    pub tags: Vec<String>,
    pub build_args: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct PushConfig {
    pub image: String,
    pub tag: String,
    pub registry: Option<String>,
    pub credentials: Option<DockerCredentials>,
}

impl DockerClient {
    pub async fn new() -> Result<Self> {
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| DockerError::Bollard(e.to_string()))?;

        // Verify connection
        docker
            .ping()
            .await
            .map_err(|_| DockerError::DaemonNotAccessible)?;

        Ok(Self { docker })
    }

    pub async fn version(&self) -> Result<String> {
        let version = self
            .docker
            .version()
            .await
            .map_err(|e| DockerError::Bollard(e.to_string()))?;

        Ok(version.version.unwrap_or_else(|| "unknown".to_string()))
    }

    pub async fn build_image<F>(&self, config: &BuildConfig, on_progress: F) -> Result<String>
    where
        F: Fn(&str),
    {
        let context_path = Path::new(&config.context);
        let tar_path = self.create_build_context(context_path).await?;

        let tar_content = {
            let mut file = File::open(&tar_path)
                .await
                .map_err(|e| DockerError::BuildFailed(format!("Failed to open tarball: {}", e)))?;
            let mut contents = Vec::new();
            file.read_to_end(&mut contents)
                .await
                .map_err(|e| DockerError::BuildFailed(format!("Failed to read tarball: {}", e)))?;
            contents
        };

        // Clean up temp tar file
        let _ = tokio::fs::remove_file(&tar_path).await;

        let primary_tag = config
            .tags
            .first()
            .cloned()
            .unwrap_or_else(|| "latest".to_string());

        let build_options = BuildImageOptions {
            dockerfile: config.dockerfile.clone(),
            t: primary_tag.clone(),
            rm: true,
            forcerm: true,
            buildargs: config.build_args.clone(),
            ..Default::default()
        };

        let mut stream = self
            .docker
            .build_image(build_options, None, Some(tar_content.into()));

        let mut image_id = String::new();
        while let Some(result) = stream.next().await {
            match result {
                Ok(output) => {
                    if let Some(stream_msg) = output.stream {
                        let msg = stream_msg.trim();
                        if !msg.is_empty() {
                            on_progress(msg);
                        }
                    }
                    if let Some(aux) = output.aux {
                        if let Some(id) = aux.id {
                            image_id = id;
                        }
                    }
                    if let Some(error) = output.error {
                        return Err(DockerError::BuildFailed(error).into());
                    }
                }
                Err(e) => {
                    return Err(DockerError::BuildFailed(e.to_string()).into());
                }
            }
        }

        // Tag with additional tags
        for tag in config.tags.iter().skip(1) {
            self.tag_image(&primary_tag, tag).await?;
        }

        Ok(image_id)
    }

    async fn create_build_context(&self, context_path: &Path) -> Result<String> {
        use std::fs::File as StdFile;
        use std::io::BufWriter;

        let tar_path =
            std::env::temp_dir().join(format!("docker_context_{}.tar", uuid::Uuid::new_v4()));

        // Create tar archive using Rust tar crate (cross-platform)
        let tar_file = StdFile::create(&tar_path)
            .map_err(|e| DockerError::BuildFailed(format!("Failed to create tar file: {}", e)))?;
        let writer = BufWriter::new(tar_file);
        let mut archive = tar::Builder::new(writer);

        // Recursively add all files from context directory
        self.add_dir_to_tar(&mut archive, context_path, Path::new(""))
            .await?;

        archive
            .finish()
            .map_err(|e| DockerError::BuildFailed(format!("Failed to finalize tar: {}", e)))?;

        Ok(tar_path.to_string_lossy().to_string())
    }

    /// Recursively adds directory contents to tar archive
    async fn add_dir_to_tar<W: std::io::Write>(
        &self,
        archive: &mut tar::Builder<W>,
        base_path: &Path,
        relative_path: &Path,
    ) -> Result<()> {
        let full_path = base_path.join(relative_path);

        let entries = std::fs::read_dir(&full_path).map_err(|e| {
            DockerError::BuildFailed(format!("Failed to read directory {:?}: {}", full_path, e))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                DockerError::BuildFailed(format!("Failed to read entry: {}", e))
            })?;

            let file_name = entry.file_name();
            let entry_relative = relative_path.join(&file_name);
            let entry_path = entry.path();

            // Skip hidden files and common ignore patterns
            let name_str = file_name.to_string_lossy();
            if name_str.starts_with('.') && name_str != ".dockerignore" {
                continue;
            }
            if name_str == "target" || name_str == "node_modules" {
                continue;
            }

            let metadata = entry.metadata().map_err(|e| {
                DockerError::BuildFailed(format!("Failed to get metadata for {:?}: {}", entry_path, e))
            })?;

            if metadata.is_dir() {
                // Recursively add directory
                Box::pin(self.add_dir_to_tar(archive, base_path, &entry_relative)).await?;
            } else if metadata.is_file() {
                // Add file to archive
                let mut file = std::fs::File::open(&entry_path).map_err(|e| {
                    DockerError::BuildFailed(format!("Failed to open file {:?}: {}", entry_path, e))
                })?;

                archive
                    .append_file(entry_relative, &mut file)
                    .map_err(|e| {
                        DockerError::BuildFailed(format!(
                            "Failed to add file {:?} to tar: {}",
                            entry_path, e
                        ))
                    })?;
            }
        }

        Ok(())
    }

    pub async fn tag_image(&self, source: &str, target: &str) -> Result<()> {
        let (repo, tag) = parse_image_tag(target);

        let options = TagImageOptions { repo, tag };

        self.docker
            .tag_image(source, Some(options))
            .await
            .map_err(|e| DockerError::TagFailed(e.to_string()))?;

        Ok(())
    }

    pub async fn push_image<F>(&self, config: &PushConfig, on_progress: F) -> Result<()>
    where
        F: Fn(&str),
    {
        let image_name = if let Some(ref registry) = config.registry {
            format!("{}/{}", registry, config.image)
        } else {
            config.image.clone()
        };

        let full_image = format!("{}:{}", image_name, config.tag);

        let options = PushImageOptions {
            tag: config.tag.clone(),
        };

        let mut stream =
            self.docker
                .push_image(&image_name, Some(options), config.credentials.clone());

        while let Some(result) = stream.next().await {
            match result {
                Ok(output) => {
                    if let Some(status) = output.status {
                        on_progress(&status);
                    }
                    if let Some(error) = output.error {
                        return Err(DockerError::PushFailed(error).into());
                    }
                }
                Err(e) => {
                    return Err(DockerError::PushFailed(e.to_string()).into());
                }
            }
        }

        on_progress(&format!("Pushed {}", full_image));
        Ok(())
    }

    pub async fn image_exists(&self, image: &str, tag: &str) -> Result<bool> {
        let full_name = format!("{}:{}", image, tag);
        match self.docker.inspect_image(&full_name).await {
            Ok(_) => Ok(true),
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => Ok(false),
            Err(e) => Err(DockerError::Bollard(e.to_string()).into()),
        }
    }
}

fn parse_image_tag(image: &str) -> (String, String) {
    if let Some((repo, tag)) = image.rsplit_once(':') {
        (repo.to_string(), tag.to_string())
    } else {
        (image.to_string(), "latest".to_string())
    }
}
