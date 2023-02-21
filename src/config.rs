use std::path::{Path, PathBuf};

const PKG_NAME: &str = "git-pr";

pub(crate) fn get_tags_path() -> String {
    let path = PathBuf::from(get_config_dir())
        .join("tags.txt");

    path.to_str().unwrap().to_string()
}

fn get_config_dir() -> String {
    if let Ok(home) = std::env::var("HOME") {
        let path = PathBuf::from(home)
            .join(".config")
            .join(PKG_NAME);

        ensure_config_dir_exists(path.to_str().unwrap());

        return path.to_str().unwrap().to_string();
    }

    panic!("Could not find home directory");
}

fn ensure_config_dir_exists(path: &str) {
    let path = Path::new(&path);
    if !path.exists() {
        std::fs::create_dir_all(path).unwrap();
    }
}