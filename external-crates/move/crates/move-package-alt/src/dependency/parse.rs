use crate::{
    errors::FileHandle,
    package::manifest::ManifestResult,
    schema::{
        DefaultDependency, EnvironmentID, EnvironmentName, ManifestDependencyInfo,
        ReplacementDependency,
    },
};

use super::{Dependency, Parsed, Pinned};

impl Dependency<Parsed> {
    /// Specialize an entry in the `[dependencies]` section, for the environment named
    /// `source_env_name` and having id `source_env_id`.
    pub fn from_default(
        file: FileHandle,
        source_env_name: EnvironmentName,
        source_env_id: EnvironmentID,
        default: DefaultDependency,
    ) -> Self {
        Dependency {
            dep_info: default.dependency_info,
            use_environment: source_env_name,
            is_override: default.is_override,
            published_at: None,
            containing_file: file,
            source_environment: source_env_id,
        }
    }

    /// Load from an entry in the `[dep-replacements]` section that has no corresponding entry in
    /// the `[dependencies]` section of the manifest. `source_env_name` and `source_env_id` refer
    /// to the environment name and ID in the original manifest; they are used as the default
    /// environment for the dependency, but will be overridden if `replacement` specifies
    /// `use-environment` field.
    pub fn from_replacement(
        file: FileHandle,
        source_env_name: EnvironmentName,
        source_env_id: EnvironmentID,
        replacement: ReplacementDependency,
    ) -> ManifestResult<Self> {
        let Some(dep) = replacement.dependency else {
            return Err(todo!());
        };

        Ok(Dependency {
            dep_info: dep.dependency_info,
            use_environment: replacement.use_environment.unwrap_or(source_env_name),
            is_override: dep.is_override,
            published_at: replacement.published_at,
            containing_file: file,
            source_environment: todo!(),
        })
    }

    pub fn from_default_with_replacement(
        file: FileHandle,
        source_env_name: EnvironmentName,
        source_env_id: EnvironmentID,
        default: DefaultDependency,
        replacement: ReplacementDependency,
    ) -> ManifestResult<Self> {
        let dep = replacement.dependency.unwrap_or(default);

        // TODO: possibly additional compatibility checks here?

        Ok(Dependency {
            dep_info: dep.dependency_info,
            use_environment: replacement.use_environment.unwrap_or(source_env_name),
            is_override: dep.is_override,
            published_at: replacement.published_at,
            containing_file: file,
            source_environment: source_env_id,
        })
    }
}
