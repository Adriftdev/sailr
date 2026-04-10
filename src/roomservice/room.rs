use checksums::{hash_file, Algorithm::BLAKE2S};
use ignore::WalkBuilder;
use std::fs::{self, File};
use std::io::prelude::*;
use std::path::{Path, PathBuf};

use super::util::fail;

#[derive(Debug)]
pub struct RoomBuilder {
    pub name: String,
    pub path: String,
    pub dependencies: Vec<String>,
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
        dependencies: Vec<String>,
        hooks: Hooks,
    ) -> RoomBuilder {
        RoomBuilder {
            name,
            path,
            dependencies,
            cache_dir,
            include,
            hooks,
            errored: false,
            should_build: true,
            latest_hash: None,
        }
    }

    fn walk_file_paths(root: &Path) -> Vec<PathBuf> {
        let builder = WalkBuilder::new(root);
        let mut files = Vec::new();

        for maybe_file in builder.build() {
            let Ok(file) = maybe_file else {
                continue;
            };

            if file.file_type().is_some_and(|entry| entry.is_file()) {
                files.push(file.path().to_path_buf());
            }
        }

        files.sort();
        files
    }

    fn generate_hash(&self, dump_scope: bool) -> String {
        let mut hash = String::with_capacity(256);
        let mut scope = String::new();
        let mut all_paths = vec![self.path.clone()];
        all_paths.extend(self.dependencies.clone());

        for directory in all_paths {
            for file_path in Self::walk_file_paths(Path::new(&directory)) {
                if dump_scope {
                    scope.push_str(file_path.to_str().unwrap());
                    scope.push('\n');
                }

                hash.push_str(&hash_file(&file_path, BLAKE2S));
                hash.push('\n');
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
        path.push('/');
        path.push_str(&self.name);

        fs::read_to_string(path).ok()
    }

    pub fn set_errored(&mut self) {
        self.errored = true;
    }

    pub fn write_hash(&self) {
        let mut path = String::new();
        path.push_str(&self.cache_dir);
        path.push('/');
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
                    self.should_build = old_hash != curr;
                }
                None => self.should_build = true,
            }
        }

        self.latest_hash = Some(curr);
    }
}
