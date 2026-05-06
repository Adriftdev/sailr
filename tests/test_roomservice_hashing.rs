use std::fs::{self, File};
use std::io::Write;
use tempfile::TempDir;
use sailr::roomservice::room::RoomBuilder;

#[test]
fn test_root_files_are_hashed_with_standard_include() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create a typical project structure
    File::create(root.join("package.json")).unwrap().write_all(b"{\"version\":\"1.0.0\"}").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    File::create(root.join("src/index.js")).unwrap().write_all(b"console.log('hello');").unwrap();

    let room = RoomBuilder::new(
        "test_room".to_string(),
        root.to_string_lossy().to_string(),
        ".roomservice".to_string(),
        vec!["./**/*.*".to_string()],
        vec![],
        vec![],
        None,
        Default::default(),
        None,
        None,
        None,
    );

    let (hash1, scoped_paths1) = room.generate_source_hash(None).unwrap();
    
    assert!(scoped_paths1.iter().any(|p| p.ends_with("package.json")), "package.json should be included in the source hash paths: {:?}", scoped_paths1);
    assert!(scoped_paths1.iter().any(|p| p.ends_with("src/index.js")), "src/index.js should be included in the source hash paths: {:?}", scoped_paths1);

    // Modify root file
    File::create(root.join("package.json")).unwrap().write_all(b"{\"version\":\"1.0.1\"}").unwrap();

    let (hash2, _) = room.generate_source_hash(None).unwrap();

    assert_ne!(hash1, hash2, "Hash should change when package.json is modified");
}
