use bbrew_core::Result;
use reqwest::Client;
use semver::Version;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Stable,
    Nightly,
}

impl Channel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Nightly => "nightly",
        }
    }
}

impl std::str::FromStr for Channel {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "stable" => Ok(Channel::Stable),
            "nightly" => Ok(Channel::Nightly),
            _ => Err(format!("Invalid channel: {}", s)),
        }
    }
}

pub struct SelfUpdater {
    client: Client,
    current_version: Version,
    channel: Channel,
    repo: String,
}

impl SelfUpdater {
    pub fn new(current_version: &str, channel: Channel, repo: Option<String>) -> Result<Self> {
        let version = Version::parse(current_version)?;
        Ok(Self {
            client: Client::new(),
            current_version: version,
            channel,
            repo: repo.unwrap_or_else(|| "metasecdev/bbrew".to_string()),
        })
    }

    /// Check for updates and return the latest version if available
    #[allow(dead_code)]
    pub async fn check_update(&self) -> Result<Option<Version>> {
        let latest_version = self.fetch_latest_version().await?;

        if latest_version > self.current_version {
            Ok(Some(latest_version))
        } else {
            Ok(None)
        }
    }

    /// Perform self-update to the latest version
    pub async fn update(&self, force: bool) -> Result<()> {
        let latest_version = self.fetch_latest_version().await?;

        if !force && latest_version <= self.current_version {
            println!("Already on the latest version ({})", self.current_version);
            return Ok(());
        }

        println!(
            "Updating from {} to {}...",
            self.current_version, latest_version
        );

        // Download and verify the new binary
        let asset_path = self.download_release(&latest_version).await?;

        // Verify the download
        self.verify_download(&asset_path, &latest_version).await?;

        // Extract binary
        let binary_path = self.extract_binary(&asset_path).await?;

        // Get current binary path
        let current_exe = std::env::current_exe()?;

        // Replace binary atomically
        self.replace_binary(&binary_path, &current_exe).await?;

        println!("Successfully updated to {}", latest_version);
        Ok(())
    }

    async fn fetch_latest_version(&self) -> Result<Version> {
        let url = match self.channel {
            Channel::Stable => {
                format!("https://api.github.com/repos/{}/releases/latest", self.repo)
            }
            Channel::Nightly => format!(
                "https://api.github.com/repos/{}/releases?per_page=1",
                self.repo
            ),
        };

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "bbrew-self-update")
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;

        let tag_name = if self.channel == Channel::Nightly {
            json[0]["tag_name"].as_str()
        } else {
            json["tag_name"].as_str()
        };

        let version_str = tag_name.ok_or_else(|| anyhow::anyhow!("Could not parse release tag"))?;
        Version::parse(version_str.trim_start_matches('v')).map_err(|e| e.into())
    }

    async fn download_release(&self, version: &Version) -> Result<PathBuf> {
        let arch = self.detect_arch()?;
        let os = self.detect_os()?;
        let asset_name = format!("bbrew-{}-{}.tar.gz", arch, os);

        // Get release assets
        let url = format!(
            "https://api.github.com/repos/{}/releases/tags/v{}",
            self.repo, version
        );
        let response = self
            .client
            .get(&url)
            .header("User-Agent", "bbrew-self-update")
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;

        // Find the matching asset
        let download_url = json["assets"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("No assets found"))?
            .iter()
            .find(|asset| asset["name"].as_str() == Some(&asset_name))
            .and_then(|asset| asset["browser_download_url"].as_str())
            .ok_or_else(|| anyhow::anyhow!("Asset not found: {}", asset_name))?;

        // Download to temp file
        let tmp_dir = std::env::temp_dir();
        let asset_path = tmp_dir.join(&asset_name);

        println!("Downloading {}...", asset_name);
        let mut response = self.client.get(download_url).send().await?;
        let mut file = tokio::fs::File::create(&asset_path).await?;

        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk).await?;
        }

        Ok(asset_path)
    }

    async fn verify_download(&self, asset_path: &PathBuf, version: &Version) -> Result<()> {
        // Verify SHA256 if checksums available
        let checksums_url = format!(
            "https://github.com/{}/releases/download/v{}/checksums.txt",
            self.repo, version
        );

        if let Ok(response) = self.client.get(&checksums_url).send().await {
            if response.status().is_success() {
                let checksums = response.text().await?;
                let asset_name = asset_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| anyhow::anyhow!("Invalid asset name"))?;

                for line in checksums.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 && parts[1] == asset_name {
                        let expected_sha256 = parts[0];
                        let actual_sha256 = self.compute_sha256(asset_path).await?;

                        if expected_sha256 != actual_sha256 {
                            return Err(anyhow::anyhow!(
                                "SHA256 mismatch: expected {}, got {}",
                                expected_sha256,
                                actual_sha256
                            ));
                        }
                        println!("SHA256 verification passed");
                        break;
                    }
                }
            }
        }

        // Try cosign verification
        self.verify_cosign(asset_path, version).await?;

        Ok(())
    }

    async fn compute_sha256(&self, path: &PathBuf) -> Result<String> {
        use sha2::{Digest, Sha256};
        use tokio::fs::File;
        use tokio::io::AsyncReadExt;

        let mut file = File::open(path).await?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];

        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    async fn verify_cosign(&self, asset_path: &Path, version: &Version) -> Result<()> {
        // Check if cosign is available
        let cosign_check = Command::new("cosign").arg("version").output().await;

        if cosign_check.is_err() {
            return Ok(()); // cosign not installed, skip
        }

        let asset_name = asset_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid asset name"))?;

        let sig_url = format!(
            "https://github.com/{}/releases/download/v{}/{}.sig",
            self.repo, version, asset_name
        );

        let pubkey_url = format!(
            "https://github.com/{}/releases/download/v{}/cosign.pub",
            self.repo, version
        );

        // Download signature
        let tmp_dir = std::env::temp_dir();
        let sig_path = tmp_dir.join(format!("{}.sig", asset_name));

        if let Ok(mut response) = self.client.get(&sig_url).send().await {
            if response.status().is_success() {
                let mut file = tokio::fs::File::create(&sig_path).await?;
                while let Some(chunk) = response.chunk().await? {
                    file.write_all(&chunk).await?;
                }

                // Verify with cosign
                let output = Command::new("cosign")
                    .args([
                        "verify-blob",
                        "--signature",
                        sig_path.to_str().unwrap(),
                        "--key",
                        &pubkey_url,
                        asset_path.to_str().unwrap(),
                    ])
                    .output()
                    .await?;

                if output.status.success() {
                    println!("Cosign signature verification passed");
                } else {
                    eprintln!(
                        "Warning: Cosign verification failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
        }

        Ok(())
    }

    async fn extract_binary(&self, asset_path: &Path) -> Result<PathBuf> {
        let tmp_dir = asset_path.parent().unwrap();
        let output = Command::new("tar")
            .args([
                "-xzf",
                asset_path.to_str().unwrap(),
                "-C",
                tmp_dir.to_str().unwrap(),
            ])
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Failed to extract archive: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let binary_path = tmp_dir.join("bbrew");
        if binary_path.exists() {
            Ok(binary_path)
        } else {
            // Try to find it
            let entries = std::fs::read_dir(tmp_dir)?;
            for entry in entries {
                let entry = entry?;
                if entry.file_name() == "bbrew" {
                    return Ok(entry.path());
                }
            }
            Err(anyhow::anyhow!("Binary not found in archive"))
        }
    }

    async fn replace_binary(&self, new_binary: &PathBuf, current_exe: &PathBuf) -> Result<()> {
        // Create backup
        let backup_path = current_exe.with_extension("bak");
        tokio::fs::copy(current_exe, &backup_path).await?;

        // Make new binary executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(new_binary).await?.permissions();
            perms.set_mode(0o755);
            tokio::fs::set_permissions(new_binary, perms).await?;
        }

        // Atomic replace using rename
        tokio::fs::rename(new_binary, current_exe).await?;

        // Remove backup on success
        let _ = tokio::fs::remove_file(&backup_path).await;

        Ok(())
    }

    fn detect_arch(&self) -> Result<String> {
        let arch = std::env::consts::ARCH;
        match arch {
            "x86_64" => Ok("x86_64".to_string()),
            "aarch64" => Ok("aarch64".to_string()),
            _ => Err(anyhow::anyhow!("Unsupported architecture: {}", arch)),
        }
    }

    fn detect_os(&self) -> Result<String> {
        let os = std::env::consts::OS;
        match os {
            "macos" => Ok("apple-darwin".to_string()),
            _ => Err(anyhow::anyhow!("Unsupported OS: {}", os)),
        }
    }
}

pub async fn self_update(current_version: &str, channel: Channel, force: bool) -> Result<()> {
    let updater = SelfUpdater::new(current_version, channel, None)?;
    updater.update(force).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_parsing() {
        assert_eq!("stable".parse::<Channel>().unwrap(), Channel::Stable);
        assert_eq!("nightly".parse::<Channel>().unwrap(), Channel::Nightly);
        assert!("invalid".parse::<Channel>().is_err());
    }

    #[test]
    fn test_channel_as_str() {
        assert_eq!(Channel::Stable.as_str(), "stable");
        assert_eq!(Channel::Nightly.as_str(), "nightly");
    }

    #[tokio::test]
    async fn test_detect_arch() {
        let updater = SelfUpdater::new("0.1.0", Channel::Stable, None).unwrap();
        let arch = updater.detect_arch().unwrap();
        assert!(arch == "x86_64" || arch == "aarch64");
    }

    #[tokio::test]
    async fn test_detect_os() {
        let updater = SelfUpdater::new("0.1.0", Channel::Stable, None).unwrap();
        let os = updater.detect_os().unwrap();
        assert_eq!(os, "apple-darwin");
    }
}
