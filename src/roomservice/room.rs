use checksums::{hash_file, Algorithm::BLAKE2S};
use ignore::{overrides::OverrideBuilder, WalkBuilder};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct Hooks {
    pub before_synchronously: Vec<String>,
    pub before: Vec<String>,
    pub run_parallel: Vec<String>,
    pub run_synchronously: Vec<String>,
    pub after: Vec<String>,
    pub finally: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RoomBuilder {
    pub name: String,
    pub path: String,
    pub dependency_rooms: Vec<String>,
    pub dependency_paths: Vec<String>,
    pub cache_dir: String,
    pub include: Vec<String>,
    pub dockerfile: Option<String>,
    pub hooks: Hooks,
    pub build_command: Option<String>,
    pub push_command: Option<String>,
    pub image_ref: Option<String>,
}

impl RoomBuilder {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        path: String,
        cache_dir: String,
        include: Vec<String>,
        dependency_rooms: Vec<String>,
        dependency_paths: Vec<String>,
        dockerfile: Option<String>,
        hooks: Hooks,
        build_command: Option<String>,
        push_command: Option<String>,
        image_ref: Option<String>,
    ) -> RoomBuilder {
        RoomBuilder {
            name,
            path,
            dependency_rooms,
            dependency_paths,
            cache_dir,
            include,
            dockerfile,
            hooks,
            build_command,
            push_command,
            image_ref,
        }
    }

    pub fn all_commands(&self) -> Vec<String> {
        let mut commands = Vec::new();
        commands.extend(self.hooks.before_synchronously.clone());
        commands.extend(self.hooks.before.clone());
        commands.extend(self.hooks.run_parallel.clone());
        commands.extend(self.hooks.run_synchronously.clone());
        commands.extend(self.build_command.clone());
        commands.extend(self.push_command.clone());
        commands.extend(self.hooks.after.clone());
        commands.extend(self.hooks.finally.clone());
        commands
    }

    pub fn generate_source_hash(
        &self,
        scope_dump_path: Option<&Path>,
    ) -> Result<(String, Vec<String>), String> {
        let mut file_hashes = Vec::new();
        let mut scoped_paths = Vec::new();

        for file_path in self.walk_file_paths(Path::new(&self.path), true)? {
            file_hashes.push(hash_file(&file_path, BLAKE2S));
            scoped_paths.push(file_path.to_string_lossy().to_string());
        }

        for dependency_path in &self.dependency_paths {
            for file_path in self.walk_file_paths(Path::new(dependency_path), false)? {
                file_hashes.push(hash_file(&file_path, BLAKE2S));
                scoped_paths.push(file_path.to_string_lossy().to_string());
            }
        }

        scoped_paths.sort();
        file_hashes.sort();

        if let Some(scope_dump_path) = scope_dump_path {
            if let Some(parent) = scope_dump_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("Failed to create scope directory: {}", error))?;
            }

            let mut file = File::create(scope_dump_path)
                .map_err(|error| format!("Failed to create scope dump: {}", error))?;
            file.write_all(scoped_paths.join("\n").as_bytes())
                .map_err(|error| format!("Failed to write scope dump: {}", error))?;
        }

        Ok((hash_text(&file_hashes.join("\n")), scoped_paths))
    }

    fn walk_file_paths(&self, root: &Path, apply_include: bool) -> Result<Vec<PathBuf>, String> {
        let mut builder = WalkBuilder::new(root);

        if apply_include && !self.include.is_empty() {
            let mut overrides = OverrideBuilder::new(root);
            for pattern in &self.include {
                let clean_pattern = pattern.trim_start_matches("./");
                overrides
                    .add(clean_pattern)
                    .map_err(|e| format!("Failed to parse include pattern '{}': {}", pattern, e))?;
            }
            let override_set = overrides
                .build()
                .map_err(|e| format!("Failed to build overrides: {}", e))?;
            builder.overrides(override_set);
        }

        let mut files = Vec::new();

        for maybe_file in builder.build() {
            let Ok(file) = maybe_file else {
                continue;
            };

            if !file.file_type().is_some_and(|entry| entry.is_file()) {
                continue;
            }

            let path = file.path().to_path_buf();
            files.push(path);
        }

        files.sort();
        Ok(files)
    }
}

pub fn hash_text(value: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
