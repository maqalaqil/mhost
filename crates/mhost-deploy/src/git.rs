use git2::Repository;
use std::path::Path;

pub struct GitOps;

impl GitOps {
    pub fn clone_repo(repo_url: &str, path: &Path, branch: &str) -> Result<(), String> {
        git2::build::RepoBuilder::new()
            .branch(branch)
            .clone(repo_url, path)
            .map_err(|e| format!("Git clone failed: {e}"))?;
        Ok(())
    }

    pub fn pull(path: &Path) -> Result<String, String> {
        let repo = Repository::open(path).map_err(|e| format!("Not a git repo: {e}"))?;
        let mut remote = repo
            .find_remote("origin")
            .map_err(|e| format!("No origin: {e}"))?;
        remote
            .fetch(&["HEAD"], None, None)
            .map_err(|e| format!("Fetch failed: {e}"))?;
        let fetch_head = repo
            .find_reference("FETCH_HEAD")
            .map_err(|e| e.to_string())?;
        let commit = fetch_head.peel_to_commit().map_err(|e| e.to_string())?;
        let commit_hash = commit.id().to_string();
        repo.reset(commit.as_object(), git2::ResetType::Hard, None)
            .map_err(|e| e.to_string())?;
        Ok(commit_hash)
    }

    pub fn current_commit(path: &Path) -> Result<String, String> {
        let repo = Repository::open(path).map_err(|e| e.to_string())?;
        let head = repo.head().map_err(|e| e.to_string())?;
        let commit = head.peel_to_commit().map_err(|e| e.to_string())?;
        Ok(commit.id().to_string())
    }

    pub fn checkout(path: &Path, commit_hash: &str) -> Result<(), String> {
        let repo = Repository::open(path).map_err(|e| e.to_string())?;
        let oid = git2::Oid::from_str(commit_hash).map_err(|e| e.to_string())?;
        let commit = repo.find_commit(oid).map_err(|e| e.to_string())?;
        repo.reset(commit.as_object(), git2::ResetType::Hard, None)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_git_repo_with_commit(dir: &Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .expect("git init");
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(dir)
            .output()
            .expect("git config email");
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(dir)
            .output()
            .expect("git config name");
        std::fs::write(dir.join("README.md"), "# test").expect("write file");
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .output()
            .expect("git add");
        Command::new("git")
            .args(["commit", "-m", "initial commit"])
            .current_dir(dir)
            .output()
            .expect("git commit");
    }

    #[test]
    fn current_commit_returns_hash() {
        let tmp = TempDir::new().unwrap();
        init_git_repo_with_commit(tmp.path());

        let hash = GitOps::current_commit(tmp.path()).expect("current_commit");
        assert_eq!(hash.len(), 40, "commit hash should be 40 chars");
    }

    #[test]
    fn current_commit_fails_on_non_repo() {
        let tmp = TempDir::new().unwrap();
        let result = GitOps::current_commit(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn checkout_restores_commit() {
        let tmp = TempDir::new().unwrap();
        init_git_repo_with_commit(tmp.path());

        let hash = GitOps::current_commit(tmp.path()).expect("current_commit");
        // checkout same commit — should succeed
        GitOps::checkout(tmp.path(), &hash).expect("checkout");

        let hash_after = GitOps::current_commit(tmp.path()).expect("current_commit after");
        assert_eq!(hash, hash_after);
    }

    #[test]
    fn commit_hash_is_hex_string() {
        let tmp = TempDir::new().unwrap();
        init_git_repo_with_commit(tmp.path());

        let hash = GitOps::current_commit(tmp.path()).expect("current_commit");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "hash should be all hex digits, got: {hash}"
        );
    }

    #[test]
    fn checkout_with_invalid_hash_fails() {
        let tmp = TempDir::new().unwrap();
        init_git_repo_with_commit(tmp.path());

        let result = GitOps::checkout(tmp.path(), "0000000000000000000000000000000000000000");
        assert!(result.is_err(), "nonexistent commit should fail checkout");
    }

    #[test]
    fn checkout_with_malformed_hash_fails() {
        let tmp = TempDir::new().unwrap();
        init_git_repo_with_commit(tmp.path());

        let result = GitOps::checkout(tmp.path(), "not-a-hash");
        assert!(result.is_err(), "malformed hash should fail checkout");
    }

    #[test]
    fn current_commit_on_second_commit_changes_hash() {
        let tmp = TempDir::new().unwrap();
        init_git_repo_with_commit(tmp.path());
        let first_hash = GitOps::current_commit(tmp.path()).expect("first commit hash");

        // Make a second commit.
        std::fs::write(tmp.path().join("file2.txt"), "second commit").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(tmp.path())
            .output()
            .expect("git add");
        Command::new("git")
            .args(["commit", "-m", "second commit"])
            .current_dir(tmp.path())
            .output()
            .expect("git commit");

        let second_hash = GitOps::current_commit(tmp.path()).expect("second commit hash");
        assert_ne!(
            first_hash, second_hash,
            "hash should differ after a new commit"
        );
        assert_eq!(second_hash.len(), 40);
    }

    #[test]
    fn checkout_to_first_commit_then_back() {
        let tmp = TempDir::new().unwrap();
        init_git_repo_with_commit(tmp.path());
        let first_hash = GitOps::current_commit(tmp.path()).expect("first hash");

        // Second commit.
        std::fs::write(tmp.path().join("extra.txt"), "extra").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(tmp.path())
            .output()
            .expect("git add");
        Command::new("git")
            .args(["commit", "-m", "second"])
            .current_dir(tmp.path())
            .output()
            .expect("git commit");

        let second_hash = GitOps::current_commit(tmp.path()).expect("second hash");
        assert_ne!(first_hash, second_hash);

        // Checkout back to first commit.
        GitOps::checkout(tmp.path(), &first_hash).expect("checkout first");
        let restored = GitOps::current_commit(tmp.path()).expect("restored hash");
        assert_eq!(restored, first_hash, "should be back at first commit");
    }
}
