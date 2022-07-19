use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use std::io::Write;

pub mod util;

struct UpgradeState {
	top_level: bool,
	handles: Vec<tokio::task::JoinHandle<()>>
}

/// Contains information about a remote directory, created from a manifest that can be fetched with [Directory::from_url].
#[derive(Serialize, Deserialize, Debug)]
pub struct Directory {
	pub name: String,
	pub files: Vec<File>,
	pub children: Vec<Directory>
}

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
	/// The first function in the callbacks tuple is called with the total number of files to download, the second is called upon finishing each download.
	/// # Warning
	/// It's up to you to get the minecraft folder right, this function deletes stuff so make sure to add some checks so users can't footgun themselves.
	pub async fn upgrade_game_folder<C1: FnOnce(usize), C2: 'static + Fn() + Send + Copy + Sync>(&self, path: &std::path::Path, total_callback: C1, download_callback: C2) {
		let mut upgrade_state = UpgradeState {
			top_level: true,
			handles: vec![]
		};

		self.upgrade_folder_to(path, &mut upgrade_state, download_callback);
		total_callback(upgrade_state.handles.len());

		for handle in upgrade_state.handles.iter_mut() {
			handle.await.unwrap();
		}
	}

	fn upgrade_folder_to<C: 'static + Fn() + Send + Copy + Sync>(&self, path: &std::path::Path, state: &mut UpgradeState, download_callback: C) {
		let mut fetch_set = std::collections::HashSet::new();
		for remote_file in &self.files {
			fetch_set.insert(remote_file);
		}
		let files = std::fs::read_dir(path).expect("cannot open path");

		match state.top_level {
			false => {
				for local_file in files.filter_map(|x| x.ok()) {
					let local_file_type = local_file.file_type().unwrap();
					if local_file_type.is_dir() {
						match self.children.iter().find(|remote_child| {
							remote_child.name == local_file.file_name().to_string_lossy()
						}) {
							Some(_) => {
								// do nothing, local folder already exists
							},
							None => {
								std::fs::remove_dir_all(local_file.path()).unwrap();
							}
						}
					} else if local_file_type.is_file() {
						let local_contents = std::fs::read(local_file.path()).unwrap();
						let local_sha = Sha256::digest(local_contents);
						match self.files.iter().find(|remote_file| {
							remote_file.sha == format!("{:x}", local_sha) &&
							remote_file.name == local_file.file_name().to_string_lossy()
						}) {
							Some(remote_file) => {
								fetch_set.remove(remote_file);
							},
							None => {
								std::fs::remove_file(local_file.path()).unwrap();
							}
						}
					}
				}
			},
			true => {
				state.top_level = false;
			}
		}

		for to_fetch in fetch_set.drain() {
			let local_path = &path.join(&to_fetch.name);
			let mut local_file = std::fs::File::create(local_path).unwrap();
			let url = to_fetch.url.clone();
			let sha = to_fetch.sha.clone();

			let fetch_handle = tokio::spawn(async move {
				loop {
					let contents = match reqwest::get(&url).await {
						Ok(contents) => contents,
						Err(reason) => {
							if reason.is_request() {
								tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
								continue;
							} else {
								panic!("{:?}", reason);
							}
						}
					}.bytes().await.unwrap();

					let downloaded_sha = format!("{:x}", Sha256::digest(&contents));
					if downloaded_sha != sha {
						panic!("sha256 for {} didn't check out\nexpected {}\nfound {}", url, sha, downloaded_sha);
					}

					local_file.write_all(&contents).unwrap();
					download_callback();
					break;
				}
			});

			state.handles.push(fetch_handle);
		}

		for child in &self.children {
			let local_path = &path.join(&child.name);

			match std::fs::create_dir(local_path) {
				Err(error) => match error.kind() {
					std::io::ErrorKind::AlreadyExists => (),
					_ => {
						panic!("cannot create folder {:?} because {}", local_path, error);
					}
				},
				_ => ()
			}

			child.upgrade_folder_to(local_path, state, download_callback);
		}
	}
}

/// Contains information about a remote file, part of a [Directory].
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
pub struct File {
	pub name: String,
	pub sha: String,
	pub url: String
}
