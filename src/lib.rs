use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use tokio::task::JoinHandle;
use tokio::sync::mpsc;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::collections::HashMap;

pub mod util;

struct UpgradeState {
	top_level: bool,
	handles: Vec<JoinHandle<()>>,
	tx: Option<mpsc::Sender<UpgradeStatus>>
}

/// Contains information about a remote directory, created from a manifest that can be fetched with [Directory::from_url].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Directory {
	pub files: HashMap<String, File>,
	pub children: HashMap<String, Directory>
}

/// A message that will give information about the status of an upgrade, note that you may recieve these events in any order (including [UpgradeStatus::Tick] before [UpgradeStatus::Length])
#[derive(Debug)]
pub enum UpgradeStatus {
	Length(usize),
	Tick
}

macro_rules! send {
	($tx:expr, $val:expr) => {
		if let Some(ref tx) = $tx {
			tx.send($val).await.unwrap()
		}
	}
}

const MAX_ATTEMPTS: u64 = 5;

impl Directory {
	/// # Description
	/// Fetches a manifest from a URL.
	/// # Warning
	/// This function will trust any URL you stick into it, ideally make sure only trusted URLs are passed in, or at least ensure all URLs use encrypted protocols like HTTPS.
	pub async fn from_url<U: reqwest::IntoUrl>(url: U) -> Option<Self> {
		let resp = reqwest::get(url).await.ok()?.text().await.ok()?;
		serde_json::from_str(&resp).ok()
	}

	/// # Description
	/// Updates a path to match the state of this instance.
	/// The sender recieves a vague indication of status through the [UpgradeStatus] enum.
	/// # Warning
	/// It's up to you to get the minecraft folder right, this function deletes stuff so make sure to add some checks so users can't footgun themselves.
	pub async fn upgrade_game_folder(&self, path: &std::path::Path, tx: Option<mpsc::Sender<UpgradeStatus>>) {
		let mut upgrade_state = UpgradeState {
			top_level: true,
			handles: vec![],
			tx
		};

		self.upgrade_folder_to(path, &mut upgrade_state).await;
		send!(upgrade_state.tx, UpgradeStatus::Length(upgrade_state.handles.len()));

		for handle in upgrade_state.handles.iter_mut() {
			handle.await.unwrap();
		}
	}

	#[async_recursion::async_recursion]
	async fn upgrade_folder_to(&self, path: &std::path::Path, state: &mut UpgradeState) {
		let mut fetch_set = self.files.clone();
		let mut files = tokio::fs::read_dir(path).await.expect("cannot open path");

		if state.top_level {
			state.top_level = false;
		} else {
			while let Ok(Some(local_file)) = files.next_entry().await {
				let local_file_type = local_file.file_type().await.unwrap();
				let local_file_name = local_file.file_name();
				let local_file_name = local_file_name.to_string_lossy();

				if local_file_type.is_dir() {
					if !self.children.contains_key(local_file_name.as_ref()) {
						tokio::fs::remove_dir_all(local_file.path()).await.unwrap();
					}
				} else if local_file_type.is_file() {
					let local_contents = tokio::fs::read(local_file.path()).await.unwrap();
					let local_sha = tokio::task::spawn_blocking(move || {
						format!("{:x}", Sha256::digest(local_contents))
					}).await.unwrap();
					match self.files.get(local_file_name.as_ref()).map(|remote_file| remote_file.sha == local_sha) {
						Some(true) => {
							fetch_set.remove(local_file_name.as_ref());
						},
						_ => {
							tokio::fs::remove_file(local_file.path()).await.unwrap();
						}
					}
				}
			}
		}

		for (name, to_fetch) in fetch_set.into_iter() {
			let local_path = &path.join(&name);
			let mut local_file = tokio::fs::File::create(local_path).await.unwrap();
			let url = to_fetch.url;
			let sha = to_fetch.sha;

			let tx = state.tx.clone();

			let fetch_handle = tokio::spawn(async move {
				let mut attempts = 0;

				loop {
					let contents = match reqwest::get(&url).await {
						Ok(contents) => contents,
						Err(reason) => {
							if reason.is_request() {
								attempts += 1;

								if attempts > MAX_ATTEMPTS {
									panic!("failed to download {url} after {MAX_ATTEMPTS} attempts");
								}

								tokio::time::sleep(tokio::time::Duration::from_millis(attempts * 250)).await;
								continue;
							} else {
								panic!("{reason:?}");
							}
						}
					}.bytes().await.unwrap();
					let contents = Arc::new(contents);

					let downloaded_sha = {
						let contents = contents.clone();
						tokio::task::spawn_blocking(move || {
							format!("{:x}", Sha256::digest(contents.as_ref()))
						}).await.unwrap()
					};
					if downloaded_sha != *sha {
						panic!("sha256 for {url} didn't check out\nexpected {sha}\nfound {downloaded_sha}");
					}

					local_file.write_all(&contents).await.unwrap();
					send!(tx, UpgradeStatus::Tick);
					break;
				}
			});

			state.handles.push(fetch_handle);
		}

		for (name, child) in &self.children {
			let local_path = &path.join(name);

			if let Err(error) = tokio::fs::create_dir(local_path).await {
				if error.kind() != std::io::ErrorKind::AlreadyExists {
					panic!("cannot create {local_path:?}: {error}");
				}
			}

			child.upgrade_folder_to(local_path, state).await;
		}
	}
}

/// Contains information about a remote file, part of a [Directory].
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
pub struct File {
	pub sha: String,
	pub url: String
}
