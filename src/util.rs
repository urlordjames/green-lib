/// Gets the default minecraft path.
/// # Warning
/// MacOS implementation is currently not tested.
pub fn minecraft_path() -> std::path::PathBuf {
	if cfg!(target_os = "windows") {
		let appdata = std::env::var("APPDATA").unwrap();
		let appdata_path = std::path::Path::new(&appdata);
		appdata_path.join(".minecraft")
	} else if cfg!(target_os = "macos") {
		let home = std::env::var("HOME").unwrap();
		let home_path = std::path::Path::new(&home);
		home_path.join("Library")
			.join("Application Support")
			.join("minecraft")
	} else {
		let home = std::env::var("HOME").unwrap();
		let home_path = std::path::Path::new(&home);
		home_path.join(".minecraft")
	}
}
