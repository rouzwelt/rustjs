use crate::error::Error;
use crate::anyhow::anyhow;
use std::path::{Path, PathBuf};
use deno_runtime::deno_core::error::AnyError;
use deno_runtime::deno_core::ModuleSpecifier;
use deno_runtime::deno_node::{NpmResolver, NodePermissions, NodeResolutionMode};

/// A very basic deno npm resolver, work with a provided node_modules path
/// which is checked against in the NpmResolver trait implementation
/// it allows read permission for all files
#[derive(Debug)]
pub struct BasicNpmResolver {
    pub node_modules_path: ModuleSpecifier,
}

impl BasicNpmResolver {
    /// creates a new instance from the given 'directory' path
    pub fn new<P: AsRef<Path>>(path: P) -> Result<BasicNpmResolver, Error> {
        Ok(BasicNpmResolver {
            node_modules_path: ModuleSpecifier::from_directory_path(path)
                .map_err(|_e| Error::FailedToParseFilePathToUrl)?,
        })
    }
}

impl NpmResolver for BasicNpmResolver {
    fn ensure_read_permission(
        &self,
        _permissions: &dyn NodePermissions,
        _path: &Path,
    ) -> Result<(), AnyError> {
        // allow all permissions
        Ok(())
    }

    fn resolve_package_folder_from_package(
        &self,
        specifier: &str,
        _referrer: &ModuleSpecifier,
        _mode: NodeResolutionMode,
    ) -> Result<PathBuf, AnyError> {
        self.node_modules_path
            .join(specifier)?
            .to_file_path()
            .map_err(|_e| anyhow!("falied to convert to file path"))
    }

    fn in_npm_package(&self, specifier: &ModuleSpecifier) -> bool {
        specifier
            .as_str()
            .starts_with(self.node_modules_path.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use deno_runtime::deno_core::anyhow;

    #[test]
    fn test_basic_npm_resolver() -> anyhow::Result<()> {
        let basic_npm_resolver = BasicNpmResolver::new("/path/to/node_modules")?;

        let test_path = ModuleSpecifier::from_file_path("/path/to/other_folder/file.js").unwrap();
        let should_not_be_in_node_modules = basic_npm_resolver.in_npm_package(&test_path);
        assert!(!should_not_be_in_node_modules);

        let test_path =
            ModuleSpecifier::from_file_path("/path/to/node_modules/some_dir/file.js").unwrap();
        let should_be_in_node_modules = basic_npm_resolver.in_npm_package(&test_path);
        assert!(should_be_in_node_modules);

        let package_folder_path = basic_npm_resolver.resolve_package_folder_from_package(
            "some_lib",
            &test_path,
            NodeResolutionMode::Execution,
        )?;
        let expected_package_folder_path =
            PathBuf::from_str("/path/to/node_modules/some_lib").unwrap();
        assert_eq!(package_folder_path, expected_package_folder_path);

        Ok(())
    }
}
