use thiserror::Error;

use crate::{
    git::{
        GitRepo,
        errors::{GitError, GitResult},
    },
    schema::{PinnedGitDependency, UnpinnedGitDependency},
};

#[derive(Error, Debug)]
pub enum PinError {
    #[error(transparent)]
    Git(#[from] GitError),
}

impl UnpinnedGitDependency {
    /// Replace the commit-ish [self.rev] with a commit (i.e. a SHA). Requires fetching the git
    /// repository
    async fn pin(&self) -> GitResult<PinnedGitDependency> {
        let git: GitRepo = self.into();
        let sha = git.find_sha().await?;

        Ok(PinnedGitDependency {
            repo: git.repo_url,
            rev: sha,
            path: git.path,
        })
    }
}

impl From<UnpinnedGitDependency> for GitRepo {
    fn from(dep: UnpinnedGitDependency) -> Self {
        GitRepo::new(dep.repo, dep.rev, dep.path)
    }
}

impl From<&UnpinnedGitDependency> for GitRepo {
    fn from(dep: &UnpinnedGitDependency) -> Self {
        GitRepo::new(dep.repo.clone(), dep.rev.clone(), dep.path.clone())
    }
}
