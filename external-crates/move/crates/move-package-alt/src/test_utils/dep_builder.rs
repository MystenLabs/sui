use crate::{package::EnvironmentName, schema::PackageName};

/// Information used to build an edge in the package graph
pub struct DepBuilder {
    /// The name that the containing package gives to the dependency
    name: PackageName,

    /// whether to include `override = true`
    is_override: bool,

    /// the `rename-from` field for the dep
    rename_from: Option<PackageName>,

    /// the `[dep-replacements]` environment to include the dep in (or `None` for the default section)
    in_env: Option<EnvironmentName>,

    /// the `use-environment` field for the dep
    use_env: Option<EnvironmentName>,
}

impl DepBuilder {
    pub fn new(name: impl AsRef<str>) -> Self {
        Self {
            name: PackageName::new(name.as_ref()).expect("valid package name"),
            is_override: false,
            rename_from: None,
            in_env: None,
            use_env: None,
        }
    }

    /// Add `override = true` to the dependency
    pub fn set_override(mut self) -> Self {
        self.is_override = true;
        self
    }

    /// Set the name used for the dependency in the containing package
    pub fn name(mut self, name: impl AsRef<str>) -> Self {
        self.name = PackageName::new(name.as_ref()).expect("valid package name");
        self
    }

    /// Set the `rename-from` field of the dependency
    pub fn rename_from(mut self, original: impl AsRef<str>) -> Self {
        self.rename_from = Some(PackageName::new(original.as_ref()).expect("valid package name"));
        self
    }

    /// Only include the dependency in `env` (in the `dep-replacements` section)
    pub fn in_env(mut self, env: impl AsRef<str>) -> Self {
        self.in_env = Some(env.as_ref().to_string());
        self
    }

    /// Set the `use-environment` field of the dependency
    pub fn use_env(mut self, env: impl AsRef<str>) -> Self {
        self.use_env = Some(env.as_ref().to_string());
        self
    }

    /// Change this to an external dependency using the mock resolver
    pub fn make_external(mut self) -> Self {
        todo!();
        self
    }
}
