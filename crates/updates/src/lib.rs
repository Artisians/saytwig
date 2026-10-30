//! Opt-in, stable-release *notifications*, not an installer. No audio, transcript,
//! credentials, telemetry, asset downloads, or unsigned code execution.
use std::{io::Read, time::Duration};

use semver::Version;
use serde::Deserialize;

pub const REPOSITORY: &str = "Artisians/saytwig";
pub const RELEASES_URL: &str = "https://github.com/Artisians/saytwig/releases";
/// Identifies fresh Saytwig releases independently of unrelated or legacy tags.
pub const RELEASE_TAG_PREFIX: &str = "saytwig-v";
const ENDPOINT: &str = "https://api.github.com/repos/Artisians/saytwig/releases/latest";
const MAX_MANIFEST: usize = 256 * 1024;

/// No trusted, static signing key or verified installer exists yet. There is
/// deliberately no runtime setting that can enable automatic installation.
pub const INSTALL_ENABLED: bool = false;
pub const INSTALL_STATUS: &str =
    "Automatic installation is unavailable: signed updates are not configured.";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Consent {
    #[default]
    Disabled,
    /// Set only after the user explicitly permits contacting GitHub.
    Enabled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckResult {
    Disabled,
    /// Also covers a repository that is not yet created or published.
    NoPublishedRelease,
    /// GitHub's latest release is not a stable release in Saytwig's namespace.
    /// This does not claim that no compatible release exists elsewhere in history.
    NoCompatibleRelease,
    UpToDate,
    Available {
        version: String,
        release_url: String,
    },
}

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum UpdateError {
    #[error("The installed version is not a valid semantic version")]
    CurrentVersion,
    #[error("Could not reach GitHub to check for updates")]
    Network,
    #[error("GitHub update checks are temporarily limited; try again later")]
    RateLimited,
    #[error("GitHub did not return a usable release")]
    Unavailable,
    #[error("The update response was not trusted")]
    Untrusted,
    #[error("The update response exceeded the size limit")]
    TooLarge,
}

/// Blocking: call on a background worker, never the UI event loop. No scheduler
/// is started here. Disabled consent performs no client construction or I/O.
pub fn check_latest(consent: Consent, current: &str) -> Result<CheckResult, UpdateError> {
    if consent == Consent::Disabled {
        return Ok(CheckResult::Disabled);
    }
    let current = Version::parse(current).map_err(|_| UpdateError::CurrentVersion)?;
    let client = reqwest::blocking::Client::builder()
        .https_only(true)
        // Refuse ALL redirects, including repository renames. An explicit
        // reviewed source change is required to alter the trust boundary.
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("Saytwig/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| UpdateError::Network)?;
    let response = client
        .get(ENDPOINT)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2026-03-10")
        .send()
        .map_err(|_| UpdateError::Network)?;
    if response.url().as_str() != ENDPOINT {
        return Err(UpdateError::Untrusted);
    }
    let status = response.status().as_u16();
    // Never read/log an error body, which could include reflected data.
    let body = if status == 200 {
        if response
            .content_length()
            .is_some_and(|n| n > MAX_MANIFEST as u64)
        {
            return Err(UpdateError::TooLarge);
        }
        bounded_body(response)?
    } else {
        Vec::new()
    };
    inspect_response(&current, status, &body)
}

fn bounded_body(reader: impl Read) -> Result<Vec<u8>, UpdateError> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_MANIFEST + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| UpdateError::Network)?;
    if bytes.len() > MAX_MANIFEST {
        return Err(UpdateError::TooLarge);
    }
    Ok(bytes)
}

fn inspect_response(
    current: &Version,
    status: u16,
    body: &[u8],
) -> Result<CheckResult, UpdateError> {
    match status {
        404 => return Ok(CheckResult::NoPublishedRelease),
        403 | 429 => return Err(UpdateError::RateLimited),
        300..=399 => return Err(UpdateError::Untrusted),
        200 => (),
        _ => return Err(UpdateError::Unavailable),
    }
    if body.len() > MAX_MANIFEST {
        return Err(UpdateError::TooLarge);
    }
    let release: Release = serde_json::from_slice(body).map_err(|_| UpdateError::Untrusted)?;
    if release.draft || release.prerelease {
        return Ok(CheckResult::NoCompatibleRelease);
    }
    if release.tag_name.len() > 128 {
        return Err(UpdateError::Untrusted);
    }
    // Never relabel an unrelated or legacy tag as a Saytwig update merely
    // because its SemVer is higher than this fresh app.
    let Some(version_text) = release.tag_name.strip_prefix(RELEASE_TAG_PREFIX) else {
        return Ok(CheckResult::NoCompatibleRelease);
    };
    let version = Version::parse(version_text).map_err(|_| UpdateError::Untrusted)?;
    if !version.pre.is_empty() {
        return Ok(CheckResult::NoCompatibleRelease);
    }
    // The suffix is constrained by semver (no paths, escapes, queries, or controls).
    // Exact matching is intentionally stricter than URL normalization.
    let release_url = format!("{RELEASES_URL}/tag/{}", release.tag_name);
    if release.html_url != release_url {
        return Err(UpdateError::Untrusted);
    }
    // Semver precedence ignores build metadata: no spurious update/downgrade.
    if !version.cmp_precedence(current).is_gt() {
        return Ok(CheckResult::UpToDate);
    }
    Ok(CheckResult::Available {
        version: version.to_string(),
        release_url,
    })
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
}
