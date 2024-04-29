use deno_runtime::deno_core::{ModuleId, ModuleSpecifier};

/// Defines the module type
/// knowing the module type is important because cjs exports
/// needs to be exposed as esm default export in order to be
/// able to interact with them in [crate::worker::JsWorker]
#[derive(Debug, Clone, PartialEq)]
pub enum JsModuleType {
    Cjs,
    Esm,
}

/// represents a js module with an id and its exported items' keys
#[derive(Debug, Clone)]
pub struct JsModule {
    pub id: ModuleId,
    pub mod_type: JsModuleType,
    pub exports: Vec<String>,
    pub url: ModuleSpecifier,
}

impl JsModule {
    pub fn export_exists(&self, key: &str) -> bool {
        self.exports.contains(&key.to_string())
    }
}

/// represents details for initializing modules on [crate::worker::JsWorker]
#[derive(Debug, Clone)]
pub struct ModuleInitializer {
    pub mod_type: JsModuleType,
    pub url: ModuleSpecifier,
}
