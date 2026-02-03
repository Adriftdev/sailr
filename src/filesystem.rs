use std::{
    error::Error,
    fs::{create_dir_all, read_dir, read_to_string, remove_dir_all, remove_file, File},
    io::Write,
    path::Path,
};

use crate::errors::{FileSystemManagerError, SailrError};

pub fn ensure_dir(dir_name: &str) -> Result<(), SailrError> {
    if !std::path::Path::new(dir_name).exists() {
        create_dir_all(dir_name)?;
    }
    Ok(())
}

pub fn rm_dir(dir_name: &str) -> Result<(), SailrError> {
    if std::path::Path::new(dir_name).exists() {
        remove_dir_all(dir_name)?;
    }
    Ok(())
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Default)]
pub struct FileSystemManager {
    pub path: String,
}

impl FileSystemManager {
    pub fn new(path: String) -> FileSystemManager {
        FileSystemManager { path }
    }

    pub fn create_file(
        &self,
        file_name: &String,
        content: &String,
    ) -> Result<(), FileSystemManagerError> {
        let path = Path::new(self.path.as_str()).join(file_name);

        match ensure_dir(path.parent().unwrap().to_str().unwrap()) {
            Ok(_) => (),
            Err(_) => {
                return Err(FileSystemManagerError::DirectoryCreationFailed(
                    path.parent().unwrap().to_str().unwrap().to_string(),
                ))
            }
        };
        let mut file = match File::create(&path) {
            Ok(file) => file,
            Err(_) => {
                return Err(FileSystemManagerError::FileWriteFailed(
                    path.as_path().to_str().unwrap().to_string(),
                ))
            }
        };
        match file.write_all(content.as_bytes()) {
            Ok(_) => (),
            Err(_) => {
                return Err(FileSystemManagerError::FileWriteFailed(
                    path.as_path().to_str().unwrap().to_string(),
                ))
            }
        };
        Ok(())
    }

    pub fn file_exists(&self, file_name: &String) -> bool {
        let path = Path::new(self.path.as_str()).join(file_name);
        path.exists()
    }

    pub fn is_dir(&self, dir_name: &String) -> bool {
        let path = Path::new(self.path.as_str()).join(dir_name);
        path.is_dir()
    }

    pub fn create_dir(&self, dir_name: &String) -> Result<(), Box<dyn Error>> {
        let path = Path::new(self.path.as_str()).join(dir_name);
        create_dir_all(path)?;
        Ok(())
    }

    pub fn read_file(
        &self,
        file_name: &String,
        dir: Option<&String>,
    ) -> Result<String, Box<dyn Error>> {
        if let Some(dir_path) = dir {
            let path = Path::new(dir_path).join(file_name);
            let contents: String = read_to_string(path)?;
            Ok(contents)
        } else {
            let path = Path::new(self.path.as_str()).join(file_name);
            let contents: String = read_to_string(path)?;
            Ok(contents)
        }
    }

    pub fn read_dir(&self, dir_name: &String) -> Result<Vec<String>, Box<dyn Error>> {
        let path = Path::new(self.path.as_str()).join(dir_name);
        let mut files = Vec::new();
        for entry in read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_str().unwrap().to_string();
            files.push(file_name);
        }
        Ok(files)
    }

    pub fn delete_file(&self, file_name: &String) -> Result<(), Box<dyn Error>> {
        let path = Path::new(self.path.as_str()).join(file_name);
        if !path.exists() {
            return Ok(());
        }
        remove_file(path)?;
        Ok(())
    }

    pub fn delete_dir(&self, dir_name: &String) -> Result<(), Box<dyn Error>> {
        let path = Path::new(self.path.as_str()).join(dir_name);
        if !path.exists() {
            return Ok(());
        }

        if path.is_file() {
            remove_file(path)?;
        } else {
            remove_dir_all(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_delete_dir_with_file() {
        // Create a temporary directory
        let dir = tempdir().unwrap();
        let fs_manager = FileSystemManager::new(dir.path().to_str().unwrap().to_string());

        // Create a file where a directory might be expected
        let file_path = dir.path().join("test_file");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "dummy content").unwrap();

        // Ensure the file exists
        assert!(file_path.exists());
        assert!(file_path.is_file());

        // Try to delete it using delete_dir
        let result = fs_manager.delete_dir(&"test_file".to_string());

        // Check that it succeeded
        assert!(result.is_ok());
        assert!(!file_path.exists());
    }
}

#[cfg(test)]
mod tests_isdir {
    use super::*;
    use std::fs::{self, File};
    use tempfile::tempdir;

    #[test]
    fn test_is_dir() {
        let dir = tempdir().unwrap();
        let fs_manager = FileSystemManager::new(dir.path().to_str().unwrap().to_string());

        // Create a directory
        let sub_dir = dir.path().join("sub_dir");
        fs::create_dir(&sub_dir).unwrap();

        // Create a file
        let file_path = dir.path().join("test_file");
        File::create(&file_path).unwrap();

        assert!(fs_manager.is_dir(&"sub_dir".to_string()));
        assert!(!fs_manager.is_dir(&"test_file".to_string()));
        assert!(!fs_manager.is_dir(&"non_existent".to_string()));
    }
}
