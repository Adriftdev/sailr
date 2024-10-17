use crate::{
    environment::Environment,
    roomservice::{
        config::RoomConfig,
        room::{Hooks, RoomBuilder},
        util::{fail, Failable},
    },
};
use std::collections::BTreeMap;

use crate::roomservice::RoomserviceBuilder;

pub struct Builder {
    roomservice: RoomserviceBuilder,
    only: Vec<String>,
    ignore: Vec<String>,
}

impl Builder {
    pub fn new(cache_dir: String, force: bool, only: Vec<String>, ignore: Vec<String>) -> Builder {
        Builder {
            roomservice: RoomserviceBuilder::new("./".to_string(), cache_dir, force),
            only,
            ignore,
        }
    }

    pub fn build(&mut self, env: &Environment) -> Result<(), String> {
        let canonical_project_path = std::path::Path::new(&"./").canonicalize().unwrap();

        let path_buf = canonical_project_path.join(".roomservice");

        let build_ignore_file = path_buf.join(".roomignore");

        let cache_dir = path_buf.to_str().unwrap().to_owned().to_string();
        let cfg = env.build.clone().unwrap_fail("No config found.");

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
                    Some(build_ignore_file.to_str().unwrap().to_owned().to_string()),
                ))
            }
        }

        self.roomservice.exec(false, false, false);
        Ok(())
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
