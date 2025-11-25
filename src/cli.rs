use crate::config::get_config_dir;

use clap::Parser;
use serde::{Deserialize, Serialize};

const DEFAULT_CONFIG_PATH: &str = "~/.config/git-pr/config.toml";

#[derive(Parser, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    #[clap(short, long, value_parser, default_value_t = false)]
    #[serde(skip_serializing, skip_deserializing)]
    pub update_only: bool,

    #[clap(short, long, value_parser, default_value_t = false)]
    #[serde(skip_serializing, skip_deserializing)]
    pub dry_run: bool,

    #[clap(short,long, value_parser, env="GIT_PR_CONFIG", default_value_t = get_config_dir())]
    pub config: String,
}
