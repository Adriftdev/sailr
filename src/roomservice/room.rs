use checksums::{hash_file, Algorithm::BLAKE2S};
use ignore::WalkBuilder;
use std::fs::{self, File};
use std::io::prelude::*;

use super::util::fail;

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
    pub ignore_file: Option<String>, // Store the path to the .roomignore file
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
        ignore_file: Option<String>, // Accept .roomignore file as an argument
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
            ignore_file, // Initialize the ignore file path
        }
    }

    fn generate_hash(&self, dump_scope: bool) -> String {
        let mut hash = String::with_capacity(256);
        let mut scope = String::new();

        // Use WalkBuilder to apply .roomignore if it exists
        let mut builder = WalkBuilder::new(&self.path);

        if let Some(ignore_file_path) = &self.ignore_file {
            // read the ignore file content and split it by new line
            // then add each line to the WalkBuilder ignore list
            let ignore_file_content =
                fs::read_to_string(ignore_file_path).expect("unable to read ignore file");
            let ignore_list: Vec<&str> = ignore_file_content.split("\n").collect();
            for ignore in ignore_list {
                builder.add_ignore(ignore);
            }
        }

        for maybe_file in builder.build() {
            let file = maybe_file.unwrap();
            println!("file: {:?}", file);
            match file.file_type() {
                Some(entry) => {
                    if entry.is_file() {
                        if dump_scope {
                            scope.push_str(file.path().to_str().unwrap());
                            scope.push_str("\n");
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
        self.errored = true;
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
        if force {
            self.should_build = true;
        } else {
            match prev {
                Some(old_hash) => {
                    if old_hash == curr {
                        self.should_build = false;
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
