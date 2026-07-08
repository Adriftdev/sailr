use runkernel::cache::{CacheEligibility, CacheManager};
use runkernel::Task;
use std::fs::{self, File};
use std::io::Write;
use tempfile::TempDir;

#[test]
fn test_root_files_are_hashed_with_standard_include() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create a typical project structure
    File::create(root.join("package.json"))
        .unwrap()
        .write_all(b"{\"version\":\"1.0.0\"}")
        .unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    File::create(root.join("src/index.js"))
        .unwrap()
        .write_all(b"console.log('hello');")
        .unwrap();

    let cache_manager = CacheManager::new();

    // We construct a Task pointing to these files
    let task = Task::new("test_task").inputs(&[
        &format!("{}/package.json", root.to_string_lossy()),
        &format!("{}/src/index.js", root.to_string_lossy()),
    ]);

    let hash1 = enabled_hash(cache_manager.compute_hash("sailr-test", &task).unwrap());

    // Modify a file
    File::create(root.join("package.json"))
        .unwrap()
        .write_all(b"{\"version\":\"1.0.1\"}")
        .unwrap();

    let hash2 = enabled_hash(cache_manager.compute_hash("sailr-test", &task).unwrap());

    assert_ne!(
        hash1, hash2,
        "Hash should change when package.json is modified"
    );
}

fn enabled_hash(eligibility: CacheEligibility) -> String {
    match eligibility {
        CacheEligibility::Enabled { hash, .. } => hash,
        CacheEligibility::Disabled(reason) => panic!("expected cache hash, got disabled: {reason}"),
    }
}
