use crate::{
    errors::FileHandle,
    package::manifest::ManifestResult,
    schema::{
        DefaultDependency, EnvironmentID, EnvironmentName, ManifestDependencyInfo,
        ReplacementDependency,
    },
};

use super::{Dependency, Parsed};

impl Dependency<Parsed> {
    pub fn from_default(
        file: FileHandle,
        environment: EnvironmentName,
        source_environment: EnvironmentID,
        default: DefaultDependency,
    ) -> Self {
        Dependency {
            dep_info: default.dependency_info,
            use_environment: environment,
            is_override: default.is_override,
            published_at: None,
            containing_file: file,
            source_environment,
        }
    }

    pub fn from_replacement(
        file: FileHandle,
        environment: EnvironmentName,
        source_environment: EnvironmentID,
        replacement: ReplacementDependency,
    ) -> ManifestResult<Self> {
        let Some(dep) = replacement.dependency else {
            return Err(todo!());
        };

        Ok(Dependency {
            dep_info: dep.dependency_info,
            use_environment: replacement.use_environment.unwrap_or(environment),
            is_override: dep.is_override,
            published_at: replacement.published_at,
            containing_file: file,
            source_environment: todo!(),
        })
    }

    pub fn from_default_with_replacement(
        file: FileHandle,
        environment: EnvironmentName,
        source_environment: EnvironmentID,
        default: DefaultDependency,
        replacement: ReplacementDependency,
    ) -> ManifestResult<Self> {
        let dep = replacement.dependency.unwrap_or(default);

        // TODO: possibly additional compatibility checks here?

        Ok(Dependency {
            dep_info: dep.dependency_info,
            use_environment: replacement.use_environment.unwrap_or(environment),
            is_override: dep.is_override,
            published_at: replacement.published_at,
            containing_file: file,
            source_environment,
        })
    }
}
