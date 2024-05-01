use crate::roomservice::util::Failable;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;

#[derive(Debug, PartialEq, Deserialize)]
pub struct Config {
    #[serde(rename = "beforeAll")]
    pub before_all: Option<String>,
    pub rooms: BTreeMap<String, RoomConfig>,
    #[serde(rename = "afterAll")]
    pub after_all: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
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

pub fn read(path_to_project: &str) -> Config {
    let mut config_contents = String::new();
    let mut file = File::open(path_to_project).unwrap_fail("Unable to open config");

    file.read_to_string(&mut config_contents)
        .expect("Error reading the config file");

    serde_yaml::from_str(&config_contents).unwrap()
}
