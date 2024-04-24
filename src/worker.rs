use crate::error::Error;
use std::{rc::Rc, sync::Arc, collections::HashMap};
use crate::resolver::BasicNpmResolver;
use deno_runtime::{
    permissions::PermissionsContainer,
    worker::{MainWorker, WorkerOptions},
    deno_core::{Extension, FsModuleLoader, ModuleId, ModuleSpecifier},
};

/// The main struct that wraps the deno js runtime and provides methods to easily load js modules
/// and interact with them
pub struct JsWorker {
    pub main_worker: MainWorker,
    pub node_modules_path: Option<ModuleSpecifier>,
    pub main_module_id: ModuleId,
    pub side_modules_ids: HashMap<String, ModuleId>,
}

impl JsWorker {
    /// creates a new instance, if no path node_modules is provided, it will default to
    /// main_module_path/node_modules
    pub async fn init(
        main_module_path: &str,
        node_modules_path: Option<&str>,
        side_modules_paths: Option<HashMap<String, String>>,
        extensions: Option<Vec<Extension>>,
    ) -> Result<JsWorker, Error> {
        let main_module = ModuleSpecifier::from_file_path(main_module_path)
            .map_err(|_| Error::FailedToParseFilePathToUrl(main_module_path.to_owned()))?;

        let mut opts_node_modules_path = None;
        let node_modules_path = if let Some(p) = node_modules_path {
            let url = ModuleSpecifier::from_file_path(p)
                .map_err(|_| Error::FailedToParseFilePathToUrl(p.to_owned()))?;
            opts_node_modules_path = Some(url.clone());
            url
        } else {
            main_module
                .join("node_modules")
                .map_err(Into::<Error>::into)?
        };

        let basic_npm_resolver = BasicNpmResolver { node_modules_path };
        let mut main_worker = MainWorker::bootstrap_from_options(
            main_module.clone(),
            PermissionsContainer::allow_all(),
            WorkerOptions {
                module_loader: Rc::new(FsModuleLoader),
                npm_resolver: Some(Arc::new(basic_npm_resolver)),
                extensions: extensions.unwrap_or_default(),
                ..Default::default()
            },
        );

        // load require and put in globalThis to be accessible by all modules
        let require_mod_id = main_worker
            .js_runtime
            .load_side_es_module_from_code(
                &ModuleSpecifier::parse("ext:__requireLoader____")?,
                format!(
                    r#"import {{ createRequire as __internalCreateRequire____ }} from "node:module";
globalThis.require = __internalCreateRequire____("{}");"#,
                    main_module.as_str(),
                ),
            )
            .await?;
        main_worker.evaluate_module(require_mod_id).await?;
        main_worker.run_event_loop(false).await?;

        // load side modules
        let mut side_modules_ids = HashMap::new();
        for (mod_name, path) in side_modules_paths.unwrap_or_default() {
            if side_modules_ids.contains_key(&mod_name) {
                return Err(Error::DuplicateSideModules(mod_name));
            }
            let side_module = ModuleSpecifier::from_file_path(&path)
                .map_err(|_| Error::FailedToParseFilePathToUrl(path.clone()))?;

            let side_mod_id = main_worker.preload_main_module(&side_module).await?;
            side_modules_ids.insert(mod_name, side_mod_id);
            main_worker.evaluate_module(side_mod_id).await?;
            main_worker.run_event_loop(false).await?;
        }

        // load main module
        let main_module_id = main_worker.preload_main_module(&main_module).await?;
        main_worker.evaluate_module(main_module_id).await?;
        main_worker.run_event_loop(false).await?;

        Ok(JsWorker {
            main_worker,
            main_module_id,
            side_modules_ids,
            node_modules_path: opts_node_modules_path,
        })
    }
}
