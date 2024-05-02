use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(rename = "beforeAll")]
    pub before_all: Option<String>,
    pub rooms: BTreeMap<String, RoomConfig>,
    #[serde(rename = "afterAll")]
    pub after_all: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomConfig {
    pub path: String,
    #[serde(default = "default_include")]
    pub include: String,
    #[serde(rename = "beforeSynchronous")]
    pub before_synchronous: Option<String>,
    pub before: Option<String>,
    #[serde(rename = "runSynchronous")]
    pub run_synchronous: Option<String>,
    #[serde(rename = "runParallel")]
    pub run_parallel: Option<String>,
    pub after: Option<String>,
    pub finally: Option<String>,
}

fn default_include() -> String {
    "./**/*.*".to_string()
}
