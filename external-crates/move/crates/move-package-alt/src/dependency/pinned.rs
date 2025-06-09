use crate::{git::errors::GitError, schema::LockfileDependencyInfo};

use super::external::ResolverError;

#[derive(Error, Debug)]
pub enum PinError {
    #[error(transparent)]
    Git(#[from] GitError),
}

impl Dependency<Pinned> {
    pub fn fetch(&self) -> FetchResult<Dependency<PackagePath>> {
        self.map(|info| match info {
            Pinned::Local(loc) => ,
            Pinned::Git(_) => todo!(),
            Pinned::OnChain(on_chain_info) => todo!(),
        })
    }
}
