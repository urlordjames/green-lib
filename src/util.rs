/// gets the default minecraft path
/// # Warning
/// no implementation for macos yet
pub fn minecraft_path() -> std::path::PathBuf {
	if cfg!(target_os = "windows") {
		let appdata = std::env::var("APPDATA").unwrap();
		let appdata_path = std::path::Path::new(&appdata);
		appdata_path.join(".minecraft")
	} else if cfg!(target_os = "macos") {
		std::path::Path::new("~/Library/Application Support/minecraft").to_path_buf()
	} else {
		let home = std::env::var("HOME").unwrap();
		let home_path = std::path::Path::new(&home);
		home_path.join(".minecraft")
	}
}
