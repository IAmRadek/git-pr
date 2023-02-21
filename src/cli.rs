use clap::Parser;
use serde::{Deserialize, Serialize};

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
}
