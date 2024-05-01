use crate::roomservice::{
    config::{read, RoomConfig},
    room::{Hooks, RoomBuilder},
    util::{fail, Failable},
};
use std::{collections::BTreeMap, path::Path};

use crate::roomservice::RoomserviceBuilder;

pub struct Builder {
    roomservice: RoomserviceBuilder,
    project: String,
    only: Vec<String>,
    ignore: Vec<String>,
}

impl Builder {
    pub fn new(
        project: String,
        cache_dir: String,
        force: bool,
        only: Vec<String>,
        ignore: Vec<String>,
    ) -> Builder {
        Builder {
            roomservice: RoomserviceBuilder::new(project.clone(), cache_dir, force),
            project,
            only,
            ignore,
        }
    }

    pub fn build(&mut self) {
        let project_path = find_config(&self.project).unwrap_fail("No config found.");
        let canonical_project_path = std::path::Path::new(&project_path).canonicalize().unwrap();

        let project_root = canonical_project_path.parent().unwrap();

        let path_buf = project_root.join(".roomservice");

        let cache_dir = path_buf.to_str().unwrap().to_owned().to_string();
        let cfg = read(&project_path);

        if cfg.before_all.is_some() {
            self.roomservice.add_before_all(&cfg.before_all.unwrap())
        }

        if cfg.after_all.is_some() {
            self.roomservice.add_after_all(&cfg.after_all.unwrap())
        }

        check_room_provided_to_flag("only".to_string(), &self.only, &cfg.rooms);

        check_room_provided_to_flag("ignore".to_string(), &self.ignore, &cfg.rooms);

        for (name, room_config) in cfg.rooms {
            let mut should_add = true;

            // @Note Check to see if it's in the only array
            if self.only.len() > 0 {
                if self.only.contains(&name) {
                    should_add = true
                } else {
                    should_add = false
                }
            }

            // @Note Check to see if it's in the ignore array
            if self.ignore.len() > 0 {
                if self.ignore.contains(&name) {
                    should_add = false
                } else {
                    should_add = true
                }
            }

            if should_add {
                self.roomservice.add_room(RoomBuilder::new(
                    name.to_string(),
                    room_config.path.to_string(),
                    cache_dir.clone(),
                    room_config.include,
                    Hooks {
                        before: room_config.before,
                        before_synchronously: room_config.before_synchronous,
                        run_synchronously: room_config.run_synchronous,
                        run_parallel: room_config.run_parallel,
                        after: room_config.after,
                        finally: room_config.finally,
                    },
                ))
            }
        }

        self.roomservice.exec(false, false, false);
    }
}

fn find_config(base_path: &str) -> Option<String> {
    if base_path.contains(".yml") {
        Some(base_path.to_string())
    } else {
        let path = Path::new(base_path);
        let maybe_config_path = Path::new(&path).join("roomservice.config.yml");

        if maybe_config_path.exists() {
            return Some(maybe_config_path.to_str().unwrap().to_string());
        } else {
            let parent = maybe_config_path.parent()?;

            if Path::new(parent).exists() {
                let relative_path = if &base_path[..2] == "./" {
                    Path::new("../").join(&base_path[2..])
                } else {
                    Path::new("../").join(base_path)
                };

                find_config(relative_path.to_str().unwrap())
            } else {
                None
            }
        }
    }
}

fn check_room_provided_to_flag(
    flag: String,
    provided_to_flag: &Vec<String>,
    rooms: &BTreeMap<String, RoomConfig>,
) {
    if provided_to_flag.len() > 0 {
        for name in provided_to_flag {
            if !rooms.keys().any(|room_name| room_name == name) {
                fail(format!(
                    "\"{}\" was provided to --{} and does not exist in config",
                    name, flag
                ))
            }
        }
    }
}

pub fn split_matches<'a>(val: Option<String>) -> Vec<String> {
    match val {
        Some(ignore_values) => ignore_values
            .split(',')
            .into_iter()
            .map(|t| t.to_string())
            .collect(),

        None => vec![],
    }
}
