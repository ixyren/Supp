//! Self-update from GitHub Releases (`ixyren/Supp`).
//!
//! Checks `https://api.github.com/repos/ixyren/Supp/releases/latest`,
//! compares semantic versions against `CARGO_PKG_VERSION`, downloads the
//! matching NSIS installer (`Supp_<ver>_x64-setup.exe`) or portable binary
//! (`Supp_Portable.exe`), and hands off to a detached helper after closing.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
pub use fastframe_update::{CHECK_INTERVAL, Installation, Kind, Release, Source, Unsupported};
use serde::Deserialize;

/// GitHub repository owning Supp's releases.
pub const REPO: &str = "ixyren/Supp";
pub const SLUG: &str = "supp";

/// Marker left by the NSIS installer next to `supp.exe`.
const INSTALLER_MARKER: &str = "supp-installer.txt";
/// Optional marker next to a portable executable.
const PORTABLE_MARKER: &str = "supp-portable.txt";

#[derive(Clone, Debug, Default)]
pub enum DownloadState {
    #[default]
    Idle,
    Downloading {
        received: u64,
        total: u64,
    },
    Ready(Box<Prepared>),
    Installing,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prepared {
    pub installation: Installation,
    pub version: String,
    pub artifact: Option<PathBuf>,
}

impl Prepared {
    /// Synthetic prepared update used by tests.
    pub fn sample(installation: Installation, version: &str) -> Self {
        Self {
            installation,
            version: version.to_owned(),
            artifact: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

/// Checks for, downloads, and stages GitHub Release updates for Supp.
#[derive(Clone, Debug)]
pub struct Updater {
    client: reqwest::blocking::Client,
    source: Source,
    repo: String,
    current_version: String,
}

impl Updater {
    pub fn new(client: reqwest::blocking::Client) -> Self {
        Self {
            client,
            source: Source::github(),
            repo: REPO.to_owned(),
            current_version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }

    pub fn with_source(mut self, source: Source) -> Self {
        self.source = source;
        self
    }

    pub fn source(&self) -> &Source {
        &self.source
    }

    pub fn check(&self) -> Result<Option<Release>> {
        let release = self.fetch_latest_release()?;
        if release.draft || release.prerelease {
            return Ok(None);
        }
        let version = normalize_version(&release.tag_name);
        if !is_newer_version(&self.current_version, version) {
            return Ok(None);
        }
        Ok(Some(Release {
            version: version.to_owned(),
            url: release.html_url,
            published_at: release.published_at,
        }))
    }

    pub fn installation(&self) -> std::result::Result<Installation, Unsupported> {
        let exe = std::env::current_exe()
            .map_err(|err| Unsupported::Unavailable(err.to_string()))?;
        installation_for_path(&exe)
    }

    pub fn download(
        &self,
        release: &Release,
        mut on_progress: impl FnMut(u64, u64),
    ) -> Result<Prepared> {
        let installation = self
            .installation()
            .map_err(|reason| anyhow::anyhow!(reason.to_string()))?;
        let gh_release = self.fetch_release_by_version(&release.version)?;
        let asset = select_asset(&gh_release.assets, installation.kind, &release.version)
            .with_context(|| {
                format!(
                    "Release v{} does not contain a matching Windows asset for {:?}",
                    release.version, installation.kind
                )
            })?;

        let staging = std::env::temp_dir().join(format!("supp-update-{}", release.version));
        std::fs::create_dir_all(&staging)
            .context("Could not create update staging directory")?;
        let dest = staging.join(&asset.name);
        let part = staging.join(format!("{}.part", asset.name));

        let mut response = self
            .client
            .get(&asset.browser_download_url)
            .header(
                reqwest::header::USER_AGENT,
                format!("Supp/{}", self.current_version),
            )
            .send()
            .context("Failed to request update download")?
            .error_for_status()
            .context("Update download returned HTTP error")?;

        let total = response.content_length().unwrap_or(asset.size);
        let mut received: u64 = 0;
        let mut out =
            std::fs::File::create(&part).context("Could not create temporary update file")?;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = response
                .read(&mut buffer)
                .context("Error reading update download stream")?;
            if read == 0 {
                break;
            }
            out.write_all(&buffer[..read])
                .context("Error writing temporary update file")?;
            received = received.saturating_add(read as u64);
            on_progress(received, total);
        }
        out.flush()?;
        drop(out);

        if dest.exists() {
            let _ = std::fs::remove_file(&dest);
        }
        std::fs::rename(&part, &dest).context("Could not finalize downloaded update file")?;

        Ok(Prepared {
            installation,
            version: release.version.clone(),
            artifact: Some(dest),
        })
    }

    pub fn handoff(&self, prepared: Prepared, _arguments: Vec<String>) -> Result<()> {
        let Some(artifact) = prepared.artifact else {
            bail!("Missing downloaded update artifact");
        };
        if !artifact.is_file() {
            bail!("Update artifact not found at {}", artifact.display());
        }
        launch_handoff(&prepared.installation, &artifact)
    }

    fn fetch_latest_release(&self) -> Result<GithubRelease> {
        let url = format!("https://api.github.com/repos/{}/releases/latest", self.repo);
        self.get_json(&url)
    }

    fn fetch_release_by_version(&self, version: &str) -> Result<GithubRelease> {
        let tag = version.trim_start_matches('v');
        let tagged_url = format!(
            "https://api.github.com/repos/{}/releases/tags/v{tag}",
            self.repo
        );
        self.get_json(&tagged_url)
            .or_else(|_| self.fetch_latest_release())
    }

    fn get_json(&self, url: &str) -> Result<GithubRelease> {
        let response = self
            .client
            .get(url)
            .header(
                reqwest::header::USER_AGENT,
                format!("Supp/{}", self.current_version),
            )
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .send()
            .with_context(|| format!("Failed to reach {url}"))?
            .error_for_status()
            .with_context(|| format!("GitHub API error for {url}"))?;
        let body = response
            .text()
            .context("Could not read GitHub Release response body")?;
        serde_json::from_str::<GithubRelease>(&body)
            .context("Could not parse GitHub Release metadata")
    }
}

fn installation_for_path(exe: &Path) -> std::result::Result<Installation, Unsupported> {
    let dir = exe
        .parent()
        .ok_or_else(|| Unsupported::Unavailable("Executable has no parent folder".into()))?;
    if dir.join(INSTALLER_MARKER).is_file() {
        return Ok(Installation {
            executable: exe.to_path_buf(),
            kind: Kind::WindowsInstaller,
        });
    }
    if dir.join(PORTABLE_MARKER).is_file() {
        return Ok(Installation {
            executable: exe.to_path_buf(),
            kind: Kind::Portable,
        });
    }
    #[cfg(windows)]
    {
        let lowered = exe.to_string_lossy().to_ascii_lowercase();
        if lowered.contains("\\windowsapps\\") {
            return Err(Unsupported::SystemDirectory);
        }
        return Ok(Installation {
            executable: exe.to_path_buf(),
            kind: Kind::Portable,
        });
    }
    #[cfg(not(windows))]
    {
        Err(Unsupported::NotPortable)
    }
}

fn select_asset<'a>(
    assets: &'a [GithubAsset],
    kind: Kind,
    version: &str,
) -> Option<&'a GithubAsset> {
    let ver = version.trim_start_matches('v');
    match kind {
        Kind::WindowsInstaller => assets
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(&format!("Supp_{ver}_x64-setup.exe")))
            .or_else(|| {
                assets
                    .iter()
                    .find(|a| a.name.to_ascii_lowercase().ends_with("-setup.exe"))
            }),
        Kind::Portable => assets
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("Supp_Portable.exe"))
            .or_else(|| {
                assets.iter().find(|a| {
                    let lower = a.name.to_ascii_lowercase();
                    lower.ends_with(".exe") && !lower.ends_with("-setup.exe")
                })
            }),
        _ => None,
    }
}

#[cfg(windows)]
fn launch_handoff(installation: &Installation, artifact: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let script_path = std::env::temp_dir().join(format!(
        "supp-apply-update-{}.cmd",
        std::process::id()
    ));
    let artifact_str = artifact.to_string_lossy().replace('"', "");
    let target_str = installation.executable.to_string_lossy().replace('"', "");

    let script = match installation.kind {
        Kind::WindowsInstaller => format!(
            "@echo off\r\n\
             ping 127.0.0.1 -n 3 >nul\r\n\
             \"{artifact_str}\" /S\r\n\
             if exist \"{target_str}\" start \"\" \"{target_str}\"\r\n\
             del \"%~f0\" >nul 2>&1\r\n"
        ),
        Kind::Portable => format!(
            "@echo off\r\n\
             ping 127.0.0.1 -n 3 >nul\r\n\
             copy /Y \"{artifact_str}\" \"{target_str}\" >nul\r\n\
             if %errorlevel%==0 (\r\n\
               del \"{artifact_str}\" >nul 2>&1\r\n\
               start \"\" \"{target_str}\"\r\n\
             )\r\n\
             del \"%~f0\" >nul 2>&1\r\n"
        ),
        other => bail!("Unsupported installation kind: {other:?}"),
    };

    std::fs::write(&script_path, script).context("Could not write updater helper script")?;
    std::process::Command::new("cmd.exe")
        .args(["/C", &script_path.to_string_lossy()])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .context("Could not start updater helper script")?;
    Ok(())
}

#[cfg(not(windows))]
fn launch_handoff(_installation: &Installation, _artifact: &Path) -> Result<()> {
    bail!("In-place installer handoff is supported on Windows builds");
}

pub fn normalize_version(tag: &str) -> &str {
    tag.trim().trim_start_matches(['v', 'V'])
}

pub fn parse_semver(tag: &str) -> Option<(u64, u64, u64)> {
    let core = normalize_version(tag).split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    match (parse_semver(current), parse_semver(candidate)) {
        (Some(cur), Some(cand)) => cand > cur,
        _ => false,
    }
}

/// An updater on the proxy-aware reqwest client.
pub fn updater() -> anyhow::Result<Updater> {
    let mut builder = reqwest::blocking::Client::builder();
    if let Some(proxy) = crate::proxy::reqwest_proxy() {
        builder = builder.proxy(proxy);
    }
    let client = builder.build().context("Could not build HTTP client")?;
    Ok(Updater::new(client))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_comparison_handles_tags_and_prereleases() {
        assert!(is_newer_version("2.0.0", "v2.0.1"));
        assert!(is_newer_version("1.0.5", "2.0.0"));
        assert!(!is_newer_version("2.0.0", "v2.0.0"));
        assert!(!is_newer_version("2.1.0", "v2.0.9"));
    }

    #[test]
    fn installer_marker_takes_priority_over_portable() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("supp.exe");
        std::fs::write(&exe, b"exe").unwrap();
        std::fs::write(dir.path().join(INSTALLER_MARKER), "installer").unwrap();
        let inst = installation_for_path(&exe).unwrap();
        assert_eq!(inst.kind, Kind::WindowsInstaller);
    }

    #[test]
    fn selects_expected_asset_for_each_kind() {
        let assets = vec![
            GithubAsset {
                name: "Supp_2.1.0_x64-setup.exe".into(),
                browser_download_url: "https://example.com/setup.exe".into(),
                size: 100,
            },
            GithubAsset {
                name: "Supp_Portable.exe".into(),
                browser_download_url: "https://example.com/portable.exe".into(),
                size: 100,
            },
        ];
        assert_eq!(
            select_asset(&assets, Kind::WindowsInstaller, "2.1.0")
                .unwrap()
                .name,
            "Supp_2.1.0_x64-setup.exe"
        );
        assert_eq!(
            select_asset(&assets, Kind::Portable, "2.1.0").unwrap().name,
            "Supp_Portable.exe"
        );
    }

    #[test]
    fn the_updater_starts_on_github() {
        assert!(updater().unwrap().source().is_github());
    }
}
