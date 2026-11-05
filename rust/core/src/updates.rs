//! Signed, product-bound update metadata. Rust never discovers GitHub `latest`
//! or the C# `beta` feed, even when the same publisher key signs both products.
use rsa::{RsaPublicKey, pkcs8::DecodePublicKey, pss, signature::Verifier, traits::PublicKeyParts};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use thiserror::Error;

pub const PRODUCT: &str = "ShovelRust";
pub const REPOSITORY: &str = "GlebHmarsky/Shovel";
pub const MAX_FEED_BYTES: usize = 1024 * 1024;
pub const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;
pub const PUBLIC_KEY: &str = include_str!("../../../distribution/update-public.pem");

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Channel {
    #[default]
    #[serde(rename = "win-rust-beta")]
    Beta,
    #[serde(rename = "win-rust-stable")]
    Stable,
}

impl Channel {
    pub fn id(self) -> &'static str {
        match self {
            Self::Beta => "win-rust-beta",
            Self::Stable => "win-rust-stable",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Beta => "Rust Beta",
            Self::Stable => "Rust Stable",
        }
    }
    pub fn branch(self) -> &'static str {
        match self {
            Self::Beta => "codex/rust-beta",
            Self::Stable => "codex/rust-stable",
        }
    }
    pub fn feed_tag(self) -> &'static str {
        match self {
            Self::Beta => "rust-beta",
            Self::Stable => "rust-stable",
        }
    }
    pub fn feed_name(self) -> String {
        format!("releases.{}.json", self.id())
    }
    pub fn feed_url(self) -> String {
        format!(
            "https://github.com/{REPOSITORY}/releases/download/{}/{}",
            self.feed_tag(),
            self.feed_name()
        )
    }
    pub fn release_tag(self, version: &Version) -> String {
        format!("rust-v{version}-{}", self.id())
    }
    pub fn package_name(self, version: &Version) -> String {
        format!("{PRODUCT}-{version}-{}-full.nupkg", self.id())
    }
    pub fn package_url(self, version: &Version) -> String {
        format!(
            "https://github.com/{REPOSITORY}/releases/download/{}/{}",
            self.release_tag(version),
            self.package_name(version)
        )
    }
    pub fn floor_name(self) -> String {
        format!("shovel-rust-trusted-version-{}", self.id())
    }
}

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("update metadata exceeds its size limit")]
    TooLarge,
    #[error("update signature verification failed")]
    Signature,
    #[error("update metadata is invalid or belongs to another product/channel")]
    InvalidFeed,
    #[error("update version is older than the previously verified version")]
    Replay,
    #[error("downloaded package does not match the signed metadata")]
    PackageMismatch,
    #[error("update data could not be read")]
    Io(#[from] std::io::Error),
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct Feed {
    schema_version: u32,
    product: String,
    channel: Channel,
    architecture: String,
    source_commit: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct Asset {
    package_id: String,
    version: Version,
    #[serde(rename = "Type")]
    kind: String,
    file_name: String,
    #[serde(rename = "SHA256")]
    sha256: String,
    #[serde(rename = "SHA1", default)]
    sha1: Option<String>,
    size: u64,
    #[serde(default)]
    notes_markdown: String,
    #[serde(default)]
    notes_html: String,
}

/// Only signature verification can construct an offered release. URLs are
/// derived from the pinned repository/product/channel, never read from a feed.
#[derive(Debug, Clone)]
pub struct VerifiedRelease {
    channel: Channel,
    source_commit: String,
    asset: Asset,
    digest: [u8; 32],
}

impl VerifiedRelease {
    pub fn channel(&self) -> Channel {
        self.channel
    }
    pub fn version(&self) -> &Version {
        &self.asset.version
    }
    pub fn source_commit(&self) -> &str {
        &self.source_commit
    }
    pub fn file_name(&self) -> &str {
        &self.asset.file_name
    }
    pub fn size(&self) -> u64 {
        self.asset.size
    }
    pub fn notes(&self) -> &str {
        &self.asset.notes_markdown
    }
    pub fn package_url(&self) -> String {
        self.channel.package_url(self.version())
    }
    pub fn is_newer_than(&self, installed: &Version) -> bool {
        self.version() > installed
    }

    /// Check exactly the signed bytes without allocating a package-sized buffer.
    pub fn verify_package(&self, mut input: impl Read) -> Result<(), UpdateError> {
        let mut hash = Sha256::new();
        let mut buffer = [0; 65536];
        let mut size = 0u64;
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            size = size
                .checked_add(count as u64)
                .ok_or(UpdateError::PackageMismatch)?;
            if size > self.size() {
                return Err(UpdateError::PackageMismatch);
            }
            hash.update(&buffer[..count]);
        }
        if size != self.size() || hash.finalize().as_slice() != self.digest {
            return Err(UpdateError::PackageMismatch);
        }
        Ok(())
    }
}

pub fn verify_feed(
    channel: Channel,
    bytes: &[u8],
    signature: &[u8],
) -> Result<VerifiedRelease, UpdateError> {
    verify_feed_with_key(channel, PUBLIC_KEY, bytes, signature)
}

fn verify_feed_with_key(
    channel: Channel,
    key: &str,
    bytes: &[u8],
    signature: &[u8],
) -> Result<VerifiedRelease, UpdateError> {
    if bytes.len() > MAX_FEED_BYTES || signature.len() > 2048 {
        return Err(UpdateError::TooLarge);
    }
    let public_key = RsaPublicKey::from_public_key_pem(key).map_err(|_| UpdateError::Signature)?;
    if public_key.n().bits() < 3072 {
        return Err(UpdateError::Signature);
    }
    let signature = pss::Signature::try_from(signature).map_err(|_| UpdateError::Signature)?;
    pss::VerifyingKey::<Sha256>::new(public_key)
        .verify(bytes, &signature)
        .map_err(|_| UpdateError::Signature)?;
    let feed: Feed = serde_json::from_slice(bytes).map_err(|_| UpdateError::InvalidFeed)?;
    if feed.schema_version != 1
        || feed.product != PRODUCT
        || feed.channel != channel
        || feed.architecture != "win-x64"
        || !matches!(feed.source_commit.len(), 40 | 64)
        || !is_hex(&feed.source_commit)
        || feed.assets.len() != 1
    {
        return Err(UpdateError::InvalidFeed);
    }
    let asset = feed
        .assets
        .into_iter()
        .next()
        .ok_or(UpdateError::InvalidFeed)?;
    if asset.package_id != PRODUCT
        || asset.kind != "Full"
        || !asset.version.pre.is_empty()
        || !asset.version.build.is_empty()
        || asset.file_name != channel.package_name(&asset.version)
        || asset.size == 0
        || asset.size > MAX_PACKAGE_BYTES
        || asset.sha256.len() != 64
        || !is_hex(&asset.sha256)
        || asset
            .sha1
            .as_ref()
            .is_some_and(|s| s.len() != 40 || !is_hex(s))
        || asset.notes_markdown.len() > 65536
        || asset.notes_html.len() > 65536
    {
        return Err(UpdateError::InvalidFeed);
    }
    let mut digest = [0; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&asset.sha256[index * 2..index * 2 + 2], 16)
            .map_err(|_| UpdateError::InvalidFeed)?;
    }
    Ok(VerifiedRelease {
        channel,
        source_commit: feed.source_commit,
        asset,
        digest,
    })
}

fn is_hex(value: &str) -> bool {
    value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    const TEST_KEY: &str = include_str!("../tests/fixtures/update-public.pem");
    const VALID: &[u8] = include_bytes!("../tests/fixtures/beta.json");
    const SIGNATURE: &[u8] = include_bytes!("../tests/fixtures/beta.json.sig");

    #[test]
    fn rust_urls_and_identifiers_are_separate() {
        let version = Version::new(0, 48, 0);
        for channel in [Channel::Beta, Channel::Stable] {
            assert!(channel.feed_url().contains(&format!(
                "/download/{}/releases.win-rust-",
                channel.feed_tag()
            )));
            assert!(!channel.feed_url().contains("/latest/"));
            assert!(
                channel
                    .package_url(&version)
                    .contains("/rust-v0.48.0-win-rust-")
            );
            assert!(channel.package_name(&version).starts_with("ShovelRust-"));
        }
        assert!(serde_json::from_str::<Channel>("\"win-beta\"").is_err());
        assert!(serde_json::from_str::<Channel>("\"win-stable\"").is_err());
    }

    #[test]
    fn verifies_powershell_pss_signature_and_exact_package() {
        let release = verify_feed_with_key(Channel::Beta, TEST_KEY, VALID, SIGNATURE).unwrap();
        assert_eq!(release.version(), &Version::new(0, 48, 0));
        assert!(
            release
                .verify_package(&b"Rust package fixture\n"[..])
                .is_ok()
        );
        assert!(
            release
                .verify_package(&b"Rust package fixturf\n"[..])
                .is_err()
        );
        assert!(release.verify_package(&b"short"[..]).is_err());
        assert!(
            release
                .verify_package(&b"Rust package fixture\nextra"[..])
                .is_err()
        );
        assert!(!release.is_newer_than(&Version::new(0, 48, 0)));
        assert!(!release.is_newer_than(&Version::new(0, 49, 0)));
        assert!(release.is_newer_than(&Version::new(0, 47, 0)));
    }

    #[test]
    fn rejects_modified_feed_wrong_key_and_swapped_channel() {
        let mut modified = VALID.to_vec();
        modified[0] ^= 1;
        assert!(verify_feed_with_key(Channel::Beta, TEST_KEY, &modified, SIGNATURE).is_err());
        assert!(verify_feed(Channel::Beta, VALID, SIGNATURE).is_err());
        assert!(verify_feed_with_key(Channel::Stable, TEST_KEY, VALID, SIGNATURE).is_err());
    }

    #[test]
    fn rejects_validly_signed_foreign_product_and_unsafe_package() {
        for (bytes, signature) in [
            (
                &include_bytes!("../tests/fixtures/csharp.json")[..],
                &include_bytes!("../tests/fixtures/csharp.json.sig")[..],
            ),
            (
                &include_bytes!("../tests/fixtures/traversal.json")[..],
                &include_bytes!("../tests/fixtures/traversal.json.sig")[..],
            ),
            (
                &include_bytes!("../tests/fixtures/wrong-architecture.json")[..],
                &include_bytes!("../tests/fixtures/wrong-architecture.json.sig")[..],
            ),
        ] {
            assert!(matches!(
                verify_feed_with_key(Channel::Beta, TEST_KEY, bytes, signature),
                Err(UpdateError::InvalidFeed)
            ));
        }
    }
}
