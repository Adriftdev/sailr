use scribe_rust::log;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

pub mod config;
pub mod room;
pub mod util;

use self::room::{hash_text, RoomBuilder};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirtyReason {
    Force,
    SourceChanged,
    DependencyChanged(String),
    CommandChanged,
    ConfigChanged,
    CacheMiss,
}

impl DirtyReason {
    pub fn describe(&self) -> String {
        match self {
            Self::Force => "forced rebuild".to_string(),
            Self::SourceChanged => "source changed".to_string(),
            Self::DependencyChanged(name) => format!("dependency changed ({})", name),
            Self::CommandChanged => "command changed".to_string(),
            Self::ConfigChanged => "config changed".to_string(),
            Self::CacheMiss => "cache miss".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoomFingerprint {
    pub source_hash: String,
    pub dependency_hash: String,
    pub command_hash: String,
    pub config_hash: String,
    pub full_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RoomCacheRecord {
    fingerprint: RoomFingerprint,
    last_outcome: String,
    last_successful_image_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhaseKind {
    BeforeAll,
    BeforeSynchronous,
    Before,
    RunParallel,
    RunSynchronous,
    Build,
    Push,
    After,
    Finally,
    AfterAll,
}

impl PhaseKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BeforeAll => "before_all",
            Self::BeforeSynchronous => "before_synchronous",
            Self::Before => "before",
            Self::RunParallel => "run_parallel",
            Self::RunSynchronous => "run_synchronous",
            Self::Build => "build",
            Self::Push => "push",
            Self::After => "after",
            Self::Finally => "finally",
            Self::AfterAll => "after_all",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PhasePlan {
    pub kind: PhaseKind,
    pub commands: Vec<String>,
    pub always_run: bool,
}

#[derive(Debug, Clone)]
pub struct RoomPlan {
    pub room: RoomBuilder,
    pub fingerprint: RoomFingerprint,
    pub phases: Vec<PhasePlan>,
    pub dirty_reasons: Vec<DirtyReason>,
    pub dirty: bool,
}

#[derive(Debug, Clone)]
pub struct BuildPlan {
    pub rooms: Vec<RoomPlan>,
    pub before_all: Vec<String>,
    pub after_all: Vec<String>,
    pub force: bool,
}

impl BuildPlan {
    pub fn print(&self, explain: bool, show_commands: bool) {
        println!("Roomservice 2.0 plan:");
        for room in &self.rooms {
            let status = if room.dirty { "dirty" } else { "clean" };
            println!(" - {} [{}]", room.room.name, status);
            if explain && !room.dirty_reasons.is_empty() {
                let reasons = room
                    .dirty_reasons
                    .iter()
                    .map(DirtyReason::describe)
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("   reasons: {}", reasons);
            }
            if show_commands && room.dirty {
                for phase in &room.phases {
                    if phase.commands.is_empty() {
                        continue;
                    }
                    println!("   phase {}:", phase.kind.as_str());
                    for command in &phase.commands {
                        println!("     {}", command);
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    Planned,
    Clean,
    Success,
    Failed,
    SkippedDependency,
}

#[derive(Debug, Clone)]
pub struct RoomExecutionResult {
    pub room_name: String,
    pub status: ExecutionStatus,
    pub dirty_reasons: Vec<DirtyReason>,
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub rooms: Vec<RoomExecutionResult>,
    pub success: bool,
    pub executed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GlobalPolicy {
    pub before_all: Vec<String>,
    pub after_all: Vec<String>,
    pub max_parallelism: Option<usize>,
    pub fail_fast: bool,
}

#[derive(Debug)]
pub struct RoomserviceBuilder {
    pub rooms: Vec<RoomBuilder>,
    project: String,
    cache_dir: PathBuf,
    force: bool,
    global_policy: GlobalPolicy,
}

impl RoomserviceBuilder {
    pub fn new(
        project: String,
        cache_dir: String,
        force: bool,
        global_policy: GlobalPolicy,
    ) -> RoomserviceBuilder {
        let cache_dir = PathBuf::from(cache_dir);
        RoomserviceBuilder {
            project,
            force,
            cache_dir,
            global_policy,
            rooms: Vec::new(),
        }
    }

    pub fn add_room(&mut self, mut room: RoomBuilder) -> Result<(), String> {
        let room_path = Path::new(&self.project).join(&room.path);
        if !room_path.exists() {
            return Err(format!(
                "Path does not exist for room \"{}\" at \"{}\"",
                room.name, room.path
            ));
        }

        room.path = room_path
            .canonicalize()
            .map_err(|error| format!("Failed to canonicalize room path: {}", error))?
            .to_string_lossy()
            .to_string();

        room.dependency_paths = room
            .dependency_paths
            .iter()
            .map(|dependency| {
                let dependency_path = Path::new(&self.project).join(dependency);
                if !dependency_path.exists() {
                    return Err(format!(
                        "Dependency path does not exist for room \"{}\" at \"{}\"",
                        room.name, dependency
                    ));
                }
                dependency_path
                    .canonicalize()
                    .map(|path| path.to_string_lossy().to_string())
                    .map_err(|error| {
                        format!(
                            "Failed to canonicalize dependency path '{}': {}",
                            dependency, error
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        self.rooms.push(room);
        Ok(())
    }

    pub fn plan(&self, dump_scope: bool) -> Result<BuildPlan, String> {
        self.ensure_cache_dirs()?;

        let ordered_rooms = self.topologically_sorted_rooms()?;
        let mut room_plans = Vec::new();
        let mut room_fingerprints: HashMap<String, RoomFingerprint> = HashMap::new();
        let mut room_dirty_state: HashMap<String, bool> = HashMap::new();

        for room in ordered_rooms {
            let scope_path = dump_scope.then(|| self.scope_dump_path(&room.name));
            let (source_hash, _) = room.generate_source_hash(scope_path.as_deref())?;
            let dependency_hash = hash_text(
                &room
                    .dependency_rooms
                    .iter()
                    .filter_map(|dependency| room_fingerprints.get(dependency))
                    .map(|fingerprint| fingerprint.full_hash.clone())
                    .chain(room.dependency_paths.iter().cloned())
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            let command_hash = hash_text(&room.all_commands().join("\n"));
            let config_hash = hash_text(
                &serde_json::to_string(&(
                    &room.path,
                    &room.include,
                    &room.dependency_rooms,
                    &room.dependency_paths,
                    &room.dockerfile,
                ))
                .map_err(|error| format!("Failed to serialize room config: {}", error))?,
            );

            let fingerprint = RoomFingerprint {
                source_hash,
                dependency_hash,
                command_hash,
                config_hash,
                full_hash: String::new(),
            };

            let fingerprint = RoomFingerprint {
                full_hash: hash_text(
                    &[
                        fingerprint.source_hash.as_str(),
                        fingerprint.dependency_hash.as_str(),
                        fingerprint.command_hash.as_str(),
                        fingerprint.config_hash.as_str(),
                    ]
                    .join("\n"),
                ),
                ..fingerprint
            };

            let cache = self.load_cache(&room.name)?;
            let legacy_hash = self.load_legacy_hash(&room.name)?;
            let mut dirty_reasons = Vec::new();

            if self.force {
                dirty_reasons.push(DirtyReason::Force);
            } else if let Some(cache) = &cache {
                if cache.fingerprint.source_hash != fingerprint.source_hash {
                    dirty_reasons.push(DirtyReason::SourceChanged);
                }
                if cache.fingerprint.command_hash != fingerprint.command_hash {
                    dirty_reasons.push(DirtyReason::CommandChanged);
                }
                if cache.fingerprint.config_hash != fingerprint.config_hash {
                    dirty_reasons.push(DirtyReason::ConfigChanged);
                }
                if cache.fingerprint.dependency_hash != fingerprint.dependency_hash {
                    dirty_reasons.push(DirtyReason::DependencyChanged("fingerprint".to_string()));
                }
            } else {
                dirty_reasons.push(DirtyReason::CacheMiss);
                if legacy_hash.as_deref() != Some(&fingerprint.source_hash) {
                    dirty_reasons.push(DirtyReason::SourceChanged);
                }
            }

            for dependency in &room.dependency_rooms {
                if room_dirty_state.get(dependency).copied().unwrap_or(false) {
                    dirty_reasons.push(DirtyReason::DependencyChanged(dependency.clone()));
                }
            }

            dedupe_dirty_reasons(&mut dirty_reasons);

            let dirty = !dirty_reasons.is_empty();
            room_dirty_state.insert(room.name.clone(), dirty);
            room_fingerprints.insert(room.name.clone(), fingerprint.clone());

            let phases = build_phases(&room);
            room_plans.push(RoomPlan {
                room,
                fingerprint,
                phases,
                dirty_reasons,
                dirty,
            });
        }

        Ok(BuildPlan {
            rooms: room_plans,
            before_all: self.global_policy.before_all.clone(),
            after_all: self.global_policy.after_all.clone(),
            force: self.force,
        })
    }

    pub fn execute(&self, plan: &BuildPlan, dry_run: bool) -> Result<ExecutionResult, String> {
        if dry_run {
            return Ok(ExecutionResult {
                rooms: plan
                    .rooms
                    .iter()
                    .map(|room| RoomExecutionResult {
                        room_name: room.room.name.clone(),
                        status: ExecutionStatus::Planned,
                        dirty_reasons: room.dirty_reasons.clone(),
                    })
                    .collect(),
                success: true,
                executed: false,
            });
        }

        self.ensure_cache_dirs()?;

        if !plan.before_all.is_empty() {
            log(scribe_rust::Color::Blue, "Executing Before All", "");
            for command in &plan.before_all {
                exec_cmd("./", command, "Before All")?;
            }
        }

        let mut results = Vec::new();
        let mut room_statuses: HashMap<String, ExecutionStatus> = HashMap::new();
        let mut overall_success = true;
        let mut abort_remaining = false;

        for room in &plan.rooms {
            if abort_remaining {
                results.push(RoomExecutionResult {
                    room_name: room.room.name.clone(),
                    status: ExecutionStatus::SkippedDependency,
                    dirty_reasons: room.dirty_reasons.clone(),
                });
                room_statuses.insert(room.room.name.clone(), ExecutionStatus::SkippedDependency);
                continue;
            }

            if room.room.dependency_rooms.iter().any(|dependency| {
                matches!(
                    room_statuses.get(dependency),
                    Some(ExecutionStatus::Failed | ExecutionStatus::SkippedDependency)
                )
            }) {
                results.push(RoomExecutionResult {
                    room_name: room.room.name.clone(),
                    status: ExecutionStatus::SkippedDependency,
                    dirty_reasons: room.dirty_reasons.clone(),
                });
                room_statuses.insert(room.room.name.clone(), ExecutionStatus::SkippedDependency);
                overall_success = false;
                continue;
            }

            if !room.dirty {
                results.push(RoomExecutionResult {
                    room_name: room.room.name.clone(),
                    status: ExecutionStatus::Clean,
                    dirty_reasons: Vec::new(),
                });
                room_statuses.insert(room.room.name.clone(), ExecutionStatus::Clean);
                continue;
            }

            let mut started = false;
            let mut room_success = true;

            for phase in room.phases.iter().filter(|phase| !phase.always_run) {
                if phase.commands.is_empty() {
                    continue;
                }
                started = true;
                log(
                    scribe_rust::Color::Blue,
                    "Executing phase",
                    &format!("{} -> {}", room.room.name, phase.kind.as_str()),
                );
                for command in &phase.commands {
                    if let Err(error) = exec_cmd(&room.room.path, command, &room.room.name) {
                        overall_success = false;
                        room_success = false;
                        log(
                            scribe_rust::Color::Red,
                            "Phase failed",
                            &format!("{} ({})", room.room.name, error),
                        );
                        break;
                    }
                }
                if !room_success {
                    break;
                }
            }

            if started {
                for phase in room.phases.iter().filter(|phase| phase.always_run) {
                    if phase.commands.is_empty() {
                        continue;
                    }
                    log(
                        scribe_rust::Color::Blue,
                        "Executing finalizer",
                        &format!("{} -> {}", room.room.name, phase.kind.as_str()),
                    );
                    for command in &phase.commands {
                        if let Err(error) = exec_cmd(&room.room.path, command, &room.room.name) {
                            overall_success = false;
                            room_success = false;
                            log(
                                scribe_rust::Color::Red,
                                "Finalizer failed",
                                &format!("{} ({})", room.room.name, error),
                            );
                            break;
                        }
                    }
                }
            }

            if room_success {
                self.write_cache(&room.room, &room.fingerprint)?;
                results.push(RoomExecutionResult {
                    room_name: room.room.name.clone(),
                    status: ExecutionStatus::Success,
                    dirty_reasons: room.dirty_reasons.clone(),
                });
                room_statuses.insert(room.room.name.clone(), ExecutionStatus::Success);
            } else {
                results.push(RoomExecutionResult {
                    room_name: room.room.name.clone(),
                    status: ExecutionStatus::Failed,
                    dirty_reasons: room.dirty_reasons.clone(),
                });
                room_statuses.insert(room.room.name.clone(), ExecutionStatus::Failed);
                if self.global_policy.fail_fast {
                    abort_remaining = true;
                }
            }
        }

        if overall_success && !plan.after_all.is_empty() {
            log(scribe_rust::Color::Blue, "Executing After All", "");
            for command in &plan.after_all {
                exec_cmd("./", command, "After All")?;
            }
        }

        Ok(ExecutionResult {
            rooms: results,
            success: overall_success,
            executed: true,
        })
    }

    fn topologically_sorted_rooms(&self) -> Result<Vec<RoomBuilder>, String> {
        let room_names = self
            .rooms
            .iter()
            .map(|room| room.name.clone())
            .collect::<HashSet<_>>();
        let mut indegree = self
            .rooms
            .iter()
            .map(|room| {
                (
                    room.name.clone(),
                    room.dependency_rooms
                        .iter()
                        .filter(|dependency| room_names.contains(*dependency))
                        .count(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
        let rooms_by_name = self
            .rooms
            .iter()
            .map(|room| (room.name.clone(), room.clone()))
            .collect::<HashMap<_, _>>();

        for room in &self.rooms {
            for dependency in room
                .dependency_rooms
                .iter()
                .filter(|dependency| room_names.contains(*dependency))
            {
                adjacency
                    .entry(dependency.clone())
                    .or_default()
                    .push(room.name.clone());
            }
        }

        let mut queue = indegree
            .iter()
            .filter_map(|(name, degree)| (*degree == 0).then_some(name.clone()))
            .collect::<VecDeque<_>>();
        let mut ordered = Vec::new();

        while let Some(name) = queue.pop_front() {
            let room = rooms_by_name
                .get(&name)
                .ok_or_else(|| format!("Missing room definition for '{}'", name))?;
            ordered.push(room.clone());

            if let Some(children) = adjacency.get(&name) {
                for child in children {
                    let degree = indegree
                        .get_mut(child)
                        .ok_or_else(|| format!("Missing indegree entry for '{}'", child))?;
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(child.clone());
                    }
                }
            }
        }

        if ordered.len() != self.rooms.len() {
            return Err("Detected circular room dependencies".to_string());
        }

        Ok(ordered)
    }

    fn ensure_cache_dirs(&self) -> Result<(), String> {
        fs::create_dir_all(self.cache_rooms_dir())
            .map_err(|error| format!("Failed to create roomservice cache directory: {}", error))?;
        fs::create_dir_all(self.cache_scopes_dir())
            .map_err(|error| format!("Failed to create roomservice scope directory: {}", error))?;
        Ok(())
    }

    fn cache_v2_dir(&self) -> PathBuf {
        self.cache_dir.join("v2")
    }

    fn cache_rooms_dir(&self) -> PathBuf {
        self.cache_v2_dir().join("rooms")
    }

    fn cache_scopes_dir(&self) -> PathBuf {
        self.cache_v2_dir().join("scopes")
    }

    fn cache_file_path(&self, room_name: &str) -> PathBuf {
        self.cache_rooms_dir().join(format!("{}.json", room_name))
    }

    fn scope_dump_path(&self, room_name: &str) -> PathBuf {
        self.cache_scopes_dir().join(format!("{}.txt", room_name))
    }

    fn load_cache(&self, room_name: &str) -> Result<Option<RoomCacheRecord>, String> {
        let path = self.cache_file_path(room_name);
        if !path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(path)
            .map_err(|error| format!("Failed to read room cache: {}", error))?;
        serde_json::from_str(&contents)
            .map(Some)
            .map_err(|error| format!("Failed to parse room cache: {}", error))
    }

    fn load_legacy_hash(&self, room_name: &str) -> Result<Option<String>, String> {
        let path = self.cache_dir.join(room_name);
        if !path.exists() || path.is_dir() {
            return Ok(None);
        }
        fs::read_to_string(path)
            .map(Some)
            .map_err(|error| format!("Failed to read legacy room cache: {}", error))
    }

    fn write_cache(&self, room: &RoomBuilder, fingerprint: &RoomFingerprint) -> Result<(), String> {
        let record = RoomCacheRecord {
            fingerprint: fingerprint.clone(),
            last_outcome: "success".to_string(),
            last_successful_image_ref: room.image_ref.clone(),
        };
        let serialized = serde_json::to_string_pretty(&record)
            .map_err(|error| format!("Failed to serialize room cache: {}", error))?;
        fs::write(self.cache_file_path(&room.name), serialized)
            .map_err(|error| format!("Failed to write room cache: {}", error))
    }
}

fn build_phases(room: &RoomBuilder) -> Vec<PhasePlan> {
    vec![
        PhasePlan {
            kind: PhaseKind::BeforeSynchronous,
            commands: room.hooks.before_synchronously.clone(),
            always_run: false,
        },
        PhasePlan {
            kind: PhaseKind::Before,
            commands: room.hooks.before.clone(),
            always_run: false,
        },
        PhasePlan {
            kind: PhaseKind::RunParallel,
            commands: room.hooks.run_parallel.clone(),
            always_run: false,
        },
        PhasePlan {
            kind: PhaseKind::RunSynchronous,
            commands: room.hooks.run_synchronously.clone(),
            always_run: false,
        },
        PhasePlan {
            kind: PhaseKind::Build,
            commands: room.build_command.clone().into_iter().collect(),
            always_run: false,
        },
        PhasePlan {
            kind: PhaseKind::Push,
            commands: room.push_command.clone().into_iter().collect(),
            always_run: false,
        },
        PhasePlan {
            kind: PhaseKind::After,
            commands: room.hooks.after.clone(),
            always_run: false,
        },
        PhasePlan {
            kind: PhaseKind::Finally,
            commands: room.hooks.finally.clone(),
            always_run: true,
        },
    ]
}

fn dedupe_dirty_reasons(reasons: &mut Vec<DirtyReason>) {
    let mut seen = HashSet::new();
    reasons.retain(|reason| seen.insert(reason.describe()));
}

fn exec_cmd(cwd: &str, cmd: &str, name: &str) -> Result<(), String> {
    use subprocess::{Exec, ExitStatus::Exited, Redirection};

    match Exec::shell(cmd)
        .cwd(cwd)
        .stdout(Redirection::Pipe)
        .stderr(Redirection::Pipe)
        .capture()
    {
        Ok(capture_data) => match capture_data.exit_status {
            Exited(0) => {
                let stdout = capture_data.stdout_str();
                if !stdout.trim().is_empty() {
                    println!("{}", stdout);
                }
                let stderr = capture_data.stderr_str();
                if !stderr.trim().is_empty() {
                    eprintln!("{}", stderr);
                }
                Ok(())
            }
            Exited(code) => {
                eprintln!(
                    "Room '{}' command failed with exit code {}: {}",
                    name, code, cmd
                );
                let stdout = capture_data.stdout_str();
                if !stdout.trim().is_empty() {
                    println!("{}", stdout);
                }
                let stderr = capture_data.stderr_str();
                if !stderr.trim().is_empty() {
                    eprintln!("{}", stderr);
                }
                Err(format!("exit {}", code))
            }
            status => Err(format!("unexpected process status: {:?}", status)),
        },
        Err(error) => Err(format!("failed to spawn command '{}': {}", cmd, error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roomservice::room::Hooks;
    use tempfile::tempdir;

    fn room(
        name: &str,
        path: &Path,
        deps: Vec<String>,
        dep_paths: Vec<String>,
        build_command: Option<String>,
        push_command: Option<String>,
        hooks: Hooks,
    ) -> RoomBuilder {
        RoomBuilder::new(
            name.to_string(),
            path.to_string_lossy().to_string(),
            ".roomservice".to_string(),
            vec!["./**/*".to_string()],
            deps,
            dep_paths,
            None,
            hooks,
            build_command,
            push_command,
            Some(format!("registry/{}:latest", name)),
        )
    }

    #[test]
    fn planner_orders_rooms_by_dependency() {
        let temp = tempdir().expect("tempdir should be created");
        let api_dir = temp.path().join("api");
        let web_dir = temp.path().join("web");
        fs::create_dir_all(&api_dir).expect("api dir should be created");
        fs::create_dir_all(&web_dir).expect("web dir should be created");
        fs::write(api_dir.join("Dockerfile"), "FROM scratch").expect("file should be created");
        fs::write(web_dir.join("Dockerfile"), "FROM scratch").expect("file should be created");

        let mut builder = RoomserviceBuilder::new(
            temp.path().to_string_lossy().to_string(),
            temp.path()
                .join(".roomservice")
                .to_string_lossy()
                .to_string(),
            false,
            GlobalPolicy::default(),
        );
        builder
            .add_room(room(
                "api",
                &api_dir,
                vec![],
                vec![],
                Some("echo api".to_string()),
                Some("echo push-api".to_string()),
                Hooks::default(),
            ))
            .expect("api room should be added");
        builder
            .add_room(room(
                "web",
                &web_dir,
                vec!["api".to_string()],
                vec![],
                Some("echo web".to_string()),
                Some("echo push-web".to_string()),
                Hooks::default(),
            ))
            .expect("web room should be added");

        let plan = builder.plan(false).expect("plan should succeed");
        assert_eq!(plan.rooms[0].room.name, "api");
        assert_eq!(plan.rooms[1].room.name, "web");
    }

    #[test]
    fn planner_marks_command_changes_dirty() {
        let temp = tempdir().expect("tempdir should be created");
        let api_dir = temp.path().join("api");
        fs::create_dir_all(&api_dir).expect("api dir should be created");
        fs::write(api_dir.join("Dockerfile"), "FROM scratch").expect("file should be created");

        let cache_root = temp.path().join(".roomservice");
        let mut builder = RoomserviceBuilder::new(
            temp.path().to_string_lossy().to_string(),
            cache_root.to_string_lossy().to_string(),
            false,
            GlobalPolicy::default(),
        );
        builder
            .add_room(room(
                "api",
                &api_dir,
                vec![],
                vec![],
                Some("echo v1".to_string()),
                Some("echo push".to_string()),
                Hooks::default(),
            ))
            .expect("room should be added");
        let initial_plan = builder.plan(false).expect("initial plan should succeed");
        builder
            .execute(&initial_plan, false)
            .expect("initial execution should succeed");

        let mut changed_builder = RoomserviceBuilder::new(
            temp.path().to_string_lossy().to_string(),
            cache_root.to_string_lossy().to_string(),
            false,
            GlobalPolicy::default(),
        );
        changed_builder
            .add_room(room(
                "api",
                &api_dir,
                vec![],
                vec![],
                Some("echo v2".to_string()),
                Some("echo push".to_string()),
                Hooks::default(),
            ))
            .expect("room should be added");
        let changed_plan = changed_builder.plan(false).expect("plan should succeed");
        assert!(changed_plan.rooms[0]
            .dirty_reasons
            .contains(&DirtyReason::CommandChanged));
    }

    #[test]
    fn execution_runs_finally_on_failure_and_skips_downstream() {
        let temp = tempdir().expect("tempdir should be created");
        let api_dir = temp.path().join("api");
        let web_dir = temp.path().join("web");
        fs::create_dir_all(&api_dir).expect("api dir should be created");
        fs::create_dir_all(&web_dir).expect("web dir should be created");
        fs::write(api_dir.join("Dockerfile"), "FROM scratch").expect("file should be created");
        fs::write(web_dir.join("Dockerfile"), "FROM scratch").expect("file should be created");
        let marker = temp.path().join("finally.txt");

        let mut builder = RoomserviceBuilder::new(
            temp.path().to_string_lossy().to_string(),
            temp.path()
                .join(".roomservice")
                .to_string_lossy()
                .to_string(),
            false,
            GlobalPolicy {
                fail_fast: false,
                ..GlobalPolicy::default()
            },
        );
        builder
            .add_room(room(
                "api",
                &api_dir,
                vec![],
                vec![],
                Some("sh -c 'exit 1'".to_string()),
                Some("echo push".to_string()),
                Hooks {
                    finally: vec![format!("sh -c 'printf done > {}'", marker.display())],
                    ..Hooks::default()
                },
            ))
            .expect("room should be added");
        builder
            .add_room(room(
                "web",
                &web_dir,
                vec!["api".to_string()],
                vec![],
                Some("echo web".to_string()),
                Some("echo push".to_string()),
                Hooks::default(),
            ))
            .expect("room should be added");

        let plan = builder.plan(false).expect("plan should succeed");
        let result = builder
            .execute(&plan, false)
            .expect("execution should return");
        assert!(!result.success);
        assert!(marker.exists());
        assert!(matches!(result.rooms[0].status, ExecutionStatus::Failed));
        assert!(matches!(
            result.rooms[1].status,
            ExecutionStatus::SkippedDependency
        ));
    }

    #[test]
    fn dry_run_does_not_execute_commands() {
        let temp = tempdir().expect("tempdir should be created");
        let api_dir = temp.path().join("api");
        fs::create_dir_all(&api_dir).expect("api dir should be created");
        fs::write(api_dir.join("Dockerfile"), "FROM scratch").expect("file should be created");
        let marker = temp.path().join("dry-run.txt");

        let mut builder = RoomserviceBuilder::new(
            temp.path().to_string_lossy().to_string(),
            temp.path()
                .join(".roomservice")
                .to_string_lossy()
                .to_string(),
            false,
            GlobalPolicy::default(),
        );
        builder
            .add_room(room(
                "api",
                &api_dir,
                vec![],
                vec![],
                Some(format!("sh -c 'printf hi > {}'", marker.display())),
                Some("echo push".to_string()),
                Hooks::default(),
            ))
            .expect("room should be added");

        let plan = builder.plan(false).expect("plan should succeed");
        let result = builder
            .execute(&plan, true)
            .expect("dry run should succeed");
        assert!(!result.executed);
        assert!(!marker.exists());
    }
}
