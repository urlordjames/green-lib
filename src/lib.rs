use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use std::io::Write;

pub mod util;

struct UpgradeState {
	top_level: bool,
	handles: Vec<tokio::task::JoinHandle<()>>
}

/// contains information about a remote directory, created from a manifest that can be fetched with [Directory::from_url]
#[derive(Serialize, Deserialize, Debug)]
pub struct Directory {
	pub name: String,
	pub files: Vec<File>,
	pub children: Vec<Directory>
}

impl Directory {
	/// # Description
	/// fetches a manifest from a URL
	/// # Warning
	/// this function will trust any URL you stick into it, ideally make sure only trusted URLs are passed in, or at least ensure all URLs use encrypted protocols like HTTPS
	pub async fn from_url<U: reqwest::IntoUrl>(url: U) -> Option<Self> {
		let resp = reqwest::get(url).await.ok()?.text().await.ok()?;
		serde_json::from_str(&resp).ok()
	}

	/// # Description
	/// updates a path to match the state of this Directory
	/// # Warning
	/// it's up to you to get the minecraft folder right, this function deletes stuff so make sure to add some checks so users can't footgun themselves
	pub async fn upgrade_game_folder(&self, path: &std::path::Path) {
		let mut upgrade_state = UpgradeState {
			top_level: true,
			handles: vec![]
		};

		self.upgrade_folder_to(path, &mut upgrade_state);

		for handle in upgrade_state.handles.iter_mut() {
			handle.await.unwrap();
		}
	}

	fn upgrade_folder_to(&self, path: &std::path::Path, state: &mut UpgradeState) {
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
								// do nothing, file already exists
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

			let fetch_handle = tokio::spawn(async move {
				let url = url;
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
					local_file.write_all(&contents).unwrap();
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

			child.upgrade_folder_to(local_path, state);
		}
	}
}

/// contains information about a remote file, part of a [Directory]
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
pub struct File {
	pub name: String,
	pub sha: String,
	pub url: String
}