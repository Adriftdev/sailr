use crate::roomservice::util::fail;
use checksums::{hash_file, Algorithm::BLAKE2S};
use ignore::Walk;
use std::fs::{self, File};
use std::io::prelude::*;

#[derive(Debug)]
pub struct RoomBuilder {
    pub name: String,
    pub path: String,
    pub cache_dir: String,
    pub include: String,
    pub hooks: Hooks,
    pub should_build: bool,
    pub latest_hash: Option<String>,
    pub errored: bool,
}

#[derive(Debug)]
pub struct Hooks {
    pub before: Option<String>,
    pub before_synchronously: Option<String>,
    pub run_synchronously: Option<String>,
    pub run_parallel: Option<String>,
    pub after: Option<String>,
    pub finally: Option<String>,
}

impl RoomBuilder {
    pub fn new(
        name: String,
        path: String,
        cache_dir: String,
        include: String,
        hooks: Hooks,
    ) -> RoomBuilder {
        RoomBuilder {
            name,
            path,
            cache_dir,
            include,
            hooks,
            errored: false,
            should_build: true,
            latest_hash: None,
        }
    }

    fn generate_hash(&self, dump_scope: bool) -> String {
        let mut hash = String::with_capacity(256);
        let mut scope = String::new();

        for maybe_file in Walk::new(&self.path) {
            let file = maybe_file.unwrap();
            match file.file_type() {
                Some(entry) => {
                    if entry.is_file() {
                        if dump_scope {
                            scope.push_str(file.path().to_str().unwrap());
                            scope.push_str("\n")
                        }

                        hash.push_str(&hash_file(file.path(), BLAKE2S));
                        hash.push_str("\n");
                    }
                }
                None => (),
            }
        }

        if dump_scope {
            fs::write(&self.name, scope).expect("unable to dump file-scope");
        }

        hash
    }

    fn prev_hash(&self) -> Option<String> {
        let mut path = String::new();
        path.push_str(&self.cache_dir);
        path.push_str("/");
        path.push_str(&self.name);

        match fs::read_to_string(path) {
            Ok(content) => Some(content),
            Err(_) => None,
        }
    }

    pub fn set_errored(&mut self) {
        self.errored = true
    }

    pub fn write_hash(&self) {
        let mut path = String::new();
        path.push_str(&self.cache_dir);
        path.push_str("/");
        path.push_str(&self.name);
        let mut file = File::create(path).unwrap();
        match file.write_all(self.latest_hash.as_ref().unwrap().as_bytes()) {
            Ok(_) => (),
            Err(_) => fail("Unable to write roomservice cache for room {}"),
        }
    }

    pub fn should_build(&mut self, force: bool, dump_scope: bool) {
        let prev = self.prev_hash();
        let curr = self.generate_hash(dump_scope);
        // println!("Current Hash is: {}, previous hash was: {:?}", curr, prev);
        if force {
            self.should_build = true
        } else {
            match prev {
                Some(old_hash) => {
                    if old_hash == curr {
                        self.should_build = false
                    } else {
                        self.should_build = true;
                    }
                }
                None => self.should_build = true,
            }
        }

        self.latest_hash = Some(curr);
    }
}
