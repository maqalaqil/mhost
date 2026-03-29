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
        let commit = fetch_head
            .peel_to_commit()
            .map_err(|e| e.to_string())?;
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
}
