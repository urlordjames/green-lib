use serde::Deserialize;
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use crate::{CLIENT, Directory};

/// Contains information about a set of [Directories](Directory).
#[derive(Deserialize, Debug, Clone)]
pub struct PacksListManifest {
	pub packs: HashMap<String, ManifestMetadata>,
	pub featured_pack: Option<String>
}

#[derive(Debug)]
pub enum FeaturedPackError {
	Unspecified,
	Invalid
}

impl PacksListManifest {
	/// # Description
	/// Fetches a packs list manifest from a URL.
	/// # Warning
	/// This function will trust any URL you stick into it, ideally make sure only trusted URLs are passed in, or at least ensure all URLs use encrypted protocols like HTTPS.
	pub async fn from_url<U: reqwest::IntoUrl>(url: U) -> Option<Self> {
		let resp = CLIENT.get(url).send().await.ok()?.text().await.ok()?;
		serde_json::from_str(&resp).ok()
	}

	/// # Description
	/// Returns the metadata of the featured pack.
	pub fn get_featured_pack_metadata(&self) -> Result<&ManifestMetadata, FeaturedPackError> {
		let featured_pack = self.featured_pack.as_ref().ok_or(FeaturedPackError::Unspecified)?;
		self.packs.get(featured_pack).ok_or(FeaturedPackError::Invalid)
	}
}

/// Contains metadata about a certain [Directory] in a [PacksListManifest].
#[derive(Deserialize, Debug, Clone)]
pub struct ManifestMetadata {
	pub display_name: String,
	manifest_url: String,
	manifest_sha: String
}

impl ManifestMetadata {
	/// # Description
	/// Fetches a manifest from its packs list metadata.
	/// The integrity of the returned [Directory] will be checked.
	/// If the integrity check fails this function will return [None].
	pub async fn to_directory(&self) -> Option<Directory> {
		let resp = CLIENT.get(&self.manifest_url).send().await.ok()?.text().await.ok()?;

		if self.manifest_sha != format!("{:x}", Sha256::digest(&resp)) {
			return None;
		}

		serde_json::from_str(&resp).ok()
	}
}
