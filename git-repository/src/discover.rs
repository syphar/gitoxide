use std::path::{Path, PathBuf};

mod path {
    use quick_error::quick_error;
    use std::path::PathBuf;

    quick_error! {
        #[derive(Debug)]
        pub enum Error {
            InaccessibleDirectory(path: PathBuf) {
                display("Failed to access a directory, or path is not a direectory")
            }
            NoGitRepository(path: PathBuf) {
                display("Could find a git repository in '{}' or in any of its parents", path.display())
            }
        }
    }
}

/// Returns the working tree if possible and the found repository is not bare or the git repository itself.
pub fn existing(directory: impl AsRef<Path>) -> Result<crate::Path, path::Error> {
    let directory = directory.as_ref();
    if !directory.is_dir() {
        return Err(path::Error::InaccessibleDirectory(directory.into()));
    }

    let mut cursor = directory;
    loop {
        if let Ok(kind) = is_git(cursor) {
            break Ok(crate::Path::from_dot_git_dir(cursor, kind));
        }
        let git_dir = cursor.join(".git");
        if let Ok(kind) = is_git(&git_dir) {
            break Ok(crate::Path::from_dot_git_dir(git_dir, kind));
        }
        match cursor.parent() {
            Some(parent) => cursor = parent,
            None => break Err(path::Error::NoGitRepository(directory.to_owned())),
        }
    }
}

pub mod is_git {
    use quick_error::quick_error;
    use std::path::PathBuf;

    quick_error! {
        #[derive(Debug)]
        pub enum Error {
            FindHeadRef(err: git_ref::file::find_one::existing::Error) {
                display("Could not find a valid HEAD reference")
                from()
                source(err)
            }
            MisplacedHead(relative_path: PathBuf) {
                display("Expected HEAD at '.git/HEAD', got '.git/{}'", relative_path.display())
            }
            MissingObjectsDirectory(missing: PathBuf) {
                display("Expected an objects directory at '{}'", missing.display())
            }
            MissingRefsDirectory(missing: PathBuf) {
                display("Expected a refs directory at '{}'", missing.display())
            }
        }
    }
}

/// What constitutes a valid git repository, and what's yet to be implemented.
///
/// * [x] a valid head
/// * [ ] git common directory
///   * [ ] respect GIT_COMMON_DIR
/// * [x] an objects directory
///   * [x] respect GIT_OBJECT_DIRECTORY
/// * [x] a refs directory
pub fn is_git(git_dir: impl AsRef<Path>) -> Result<crate::Kind, is_git::Error> {
    let dot_git = git_dir.as_ref();

    {
        let refs = git_ref::file::Store::at(&dot_git);
        let head = refs.find_one_existing("HEAD")?;
        if head.relative_path != Path::new("HEAD") {
            return Err(is_git::Error::MisplacedHead(head.relative_path));
        }
    }

    {
        let objects_path = std::env::var("GIT_OBJECT_DIRECTORY")
            .map(PathBuf::from)
            .unwrap_or_else(|_| dot_git.join("objects"));
        if !objects_path.is_dir() {
            return Err(is_git::Error::MissingObjectsDirectory(objects_path));
        }
    }
    {
        let refs_path = dot_git.join("refs");
        if !refs_path.is_dir() {
            return Err(is_git::Error::MissingRefsDirectory(refs_path));
        }
    }

    Ok(if dot_git.join("index").is_file() {
        crate::Kind::WorkingTree
    } else {
        crate::Kind::Bare
    })
}
