use crate::{
    environment::Environment,
    roomservice::{
        room::{Hooks, RoomBuilder},
        util::Failable,
    },
};

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

        let cache_dir = path_buf.to_str().unwrap().to_owned().to_string();
        let cfg = env.build.clone().unwrap_fail("No config found.");

        if let Some(before_all) = cfg.before_all {
            self.roomservice.add_before_all(&before_all)
        }

        if let Some(after_all) = cfg.after_all {
            self.roomservice.add_after_all(&after_all)
        }

        //check_room_provided_to_flag("only".to_string(), &self.only, &cfg.rooms);

        //check_room_provided_to_flag("ignore".to_string(), &self.ignore, &cfg.rooms);

        for (name, room_config) in cfg.rooms {
            let mut should_add = true;

            // @Note Check to see if it's in the only array
            if !self.only.is_empty() {
                should_add = self.only.contains(&name);
            }

            // @Note Check to see if it's in the ignore array
            if !self.ignore.is_empty() {
                should_add = !self.ignore.contains(&name);
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
        Ok(())
    }
}

pub fn split_matches(val: Option<String>) -> Vec<String> {
    match val {
        Some(ignore_values) => ignore_values.split(',').map(|t| t.to_string()).collect(),

        None => vec![],
    }
}
