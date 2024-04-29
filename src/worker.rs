use std::{rc::Rc, sync::Arc};
use crate::resolver::BasicNpmResolver;
use crate::{
    error::{Error, catch_exception},
    module::{JsModule, ModuleInitializer, JsModuleType},
};
use deno_runtime::{
    deno_core::{v8, serde_v8::from_v8, Extension, FsModuleLoader, ModuleSpecifier},
    deno_napi::v8::GetPropertyNamesArgs,
    permissions::PermissionsContainer,
    worker::{MainWorker, WorkerOptions},
};

/// options for instantiating a [JsWorker]
#[derive(Debug, Clone)]
pub struct JsWorkerInitOptions {
    pub main_module_initializer: ModuleInitializer,
    pub node_modules_url: Option<ModuleSpecifier>,
}

/// The main struct that wraps the deno js runtime and provides methods to easily load js modules
/// and interact with them
pub struct JsWorker {
    pub(crate) main_worker: MainWorker,
    pub(crate) main_module: JsModule,
    pub(crate) node_modules_url: Option<ModuleSpecifier>,
}

impl JsWorker {
    /// get main worker [MainWorker] of this instance
    pub fn main_worker(&mut self) -> &mut MainWorker {
        &mut self.main_worker
    }

    /// get main module [JsModule] of this instance
    pub fn main_module(&self) -> &JsModule {
        &self.main_module
    }

    /// get node_modules url of this instance
    pub fn node_modules_url(&self) -> Option<ModuleSpecifier> {
        self.node_modules_url.clone()
    }

    /// creates a new instance, if no path node_modules is provided, it will default to
    /// main_module_path/node_modules
    pub async fn init(
        options: JsWorkerInitOptions,
        extensions: Option<Vec<Extension>>,
    ) -> Result<JsWorker, Error> {
        let node_modules_path = if let Some(p) = &options.node_modules_url {
            p.clone()
        } else {
            options
                .main_module_initializer
                .url
                .join("..")?
                .join("node_modules")?
        };

        let basic_npm_resolver = BasicNpmResolver {
            node_modules_url: node_modules_path,
        };
        let mut main_worker = MainWorker::bootstrap_from_options(
            options.main_module_initializer.url.clone(),
            PermissionsContainer::allow_all(),
            WorkerOptions {
                module_loader: Rc::new(FsModuleLoader),
                npm_resolver: Some(Arc::new(basic_npm_resolver)),
                extensions: extensions.unwrap_or_default(),
                ..Default::default()
            },
        );

        // load main module
        let main_module_id = if options.main_module_initializer.mod_type == JsModuleType::Esm {
            main_worker
                .preload_main_module(&options.main_module_initializer.url)
                .await?
        } else {
            // load require and put in globalThis to be accessible by all cjs modules
            let require_mod_id = main_worker
                .js_runtime
                .load_side_es_module_from_code(
                    &ModuleSpecifier::parse("ext:__requireLoader____")?,
                    format!(
                        r#"import {{ createRequire as __internalCreateRequire____ }} from "node:module";
globalThis.require = __internalCreateRequire____("{}");"#,
                        options.main_module_initializer.url.as_str(),
                    ),
                )
                .await?;
            main_worker.evaluate_module(require_mod_id).await?;

            main_worker
                .js_runtime
                .load_side_es_module_from_code(
                    &ModuleSpecifier::parse("ext:__cjsMainModuleExporter____")?,
                    format!(
                        r#"const __moduleExports____ = require("{}"); export default __moduleExports____;"#,
                        options.main_module_initializer.url.path()
                    ),
                )
                .await?
        };
        main_worker.evaluate_module(main_module_id).await?;

        // run eventloop to finish
        main_worker.run_event_loop(false).await?;

        // get export keys of main module
        let exports = {
            let module = main_worker
                .js_runtime
                .get_module_namespace(main_module_id)?;
            let mut scope = main_worker.js_runtime.handle_scope();
            let module = module.open(&mut scope);
            if options.main_module_initializer.mod_type == JsModuleType::Esm {
                let names = module.get_property_names(&mut scope, GetPropertyNamesArgs::default());
                if let Some(v) = names {
                    from_v8::<Vec<String>>(&mut scope, v.into())?
                } else {
                    vec![]
                }
            } else {
                let mut all_exports = vec!["default".to_string()];
                let default_key = v8::String::new(&mut scope, "default")
                    .ok_or(Error::FailedToGetV8Value)?
                    .into();
                let default_export = module
                    .get(&mut scope, default_key)
                    .ok_or(Error::FailedToGetV8Value)?
                    .to_object(&mut scope)
                    .ok_or(Error::FailedToGetV8Value)?;
                let inner_exports = default_export
                    .get_property_names(&mut scope, GetPropertyNamesArgs::default())
                    .ok_or(Error::FailedToGetV8Value)?;
                let inner_exports = from_v8::<Vec<String>>(&mut scope, inner_exports.into())?;
                all_exports.extend_from_slice(&inner_exports);
                all_exports
            }
        };

        Ok(JsWorker {
            main_worker,
            node_modules_url: options.node_modules_url,
            main_module: JsModule {
                id: main_module_id,
                mod_type: options.main_module_initializer.mod_type,
                exports,
                url: options.main_module_initializer.url,
            },
        })
    }

    /// get module object instance
    pub fn get_main_module_instance(&mut self) -> Result<v8::Global<v8::Object>, Error> {
        let module = self
            .main_worker
            .js_runtime
            .get_module_namespace(self.main_module.id)?;
        Ok(module)
    }

    /// get the export value
    pub fn get_export(&mut self, name: &str) -> Result<v8::Global<v8::Value>, Error> {
        if !self.main_module.export_exists(name) {
            return Err(Error::UndefinedExport);
        }

        let mut module = self.get_main_module_instance()?;
        let scope = &mut self.main_worker.js_runtime.handle_scope();

        if self.main_module.mod_type == JsModuleType::Cjs {
            let module_instance = module.open(scope);
            let default_key = v8::String::new(scope, "default").ok_or(Error::FailedToGetV8Value)?;
            let default_export = module_instance
                .get(scope, default_key.into())
                .ok_or(Error::FailedToGetV8Value)?;

            let default_export = default_export
                .to_object(scope)
                .ok_or(Error::FailedToGetV8Value)?;

            module = v8::Global::new(scope, default_export);
        }

        let module = module.open(scope);
        let key = v8::String::new(scope, name).ok_or(Error::FailedToGetV8Value)?;
        let value = module
            .get(scope, key.into())
            .ok_or(Error::FailedToGetV8Value)?;

        Ok(v8::Global::new(scope, value))
    }

    /// call js function
    pub fn call_fn(
        &mut self,
        name: &str,
        args: &[v8::Global<v8::Value>],
    ) -> Result<v8::Global<v8::Value>, Error> {
        let module = self.get_main_module_instance()?;
        let function = self.get_export(name)?;

        let scope = &mut self.main_worker.js_runtime.handle_scope();
        let scope = &mut v8::TryCatch::new(scope);

        let function: v8::Local<v8::Function> = v8::Local::new(scope, function).try_into()?;
        let module = v8::Local::new(scope, module);
        let mut local_args = vec![];
        args.iter()
            .for_each(|v| local_args.push(v8::Local::new(scope, v)));
        let result = function.call(scope, module.into(), &local_args);

        if let Some(v) = result {
            Ok(v8::Global::new(scope, v))
        } else {
            Err(catch_exception(scope))
        }
    }

    /// call an async js function and get the resolved/rejected results
    pub async fn call_async_fn(
        &mut self,
        name: &str,
        args: &[v8::Global<v8::Value>],
    ) -> Result<v8::Global<v8::Value>, Error> {
        let res = self.call_fn(name, args)?;
        let future = self.main_worker.js_runtime.resolve(res);
        Ok(self
            .main_worker
            .js_runtime
            .with_event_loop_future(future, Default::default())
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anyhow;

    #[tokio::test]
    async fn test_init_esm() -> anyhow::Result<()> {
        let options = JsWorkerInitOptions {
            main_module_initializer: ModuleInitializer {
                mod_type: JsModuleType::Esm,
                url: ModuleSpecifier::from_file_path(
                    std::env::current_dir().unwrap().join("data/esm.js"),
                )
                .unwrap(),
            },
            node_modules_url: None,
        };
        let js_worker = JsWorker::init(options, None).await?;
        let expected_exported_modules_keys = vec![
            "asyncFnReject".to_string(),
            "asyncFnResolve".to_string(),
            "fnWithError".to_string(),
            "topFn".to_string(),
        ];

        assert_eq!(
            js_worker.main_module.exports,
            expected_exported_modules_keys
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_init_cjs() -> anyhow::Result<()> {
        let options = JsWorkerInitOptions {
            main_module_initializer: ModuleInitializer {
                mod_type: JsModuleType::Cjs,
                url: ModuleSpecifier::from_file_path(
                    std::env::current_dir().unwrap().join("data/cjs.js"),
                )
                .unwrap(),
            },
            node_modules_url: None,
        };
        let js_worker = JsWorker::init(options, None).await?;

        // cjs modules always have default exports
        let expected_exported_modules_keys = vec![
            "default".to_string(),
            "topFn".to_string(),
            "asyncFnResolve".to_string(),
            "asyncFnReject".to_string(),
            "fnWithError".to_string(),
        ];
        assert_eq!(
            js_worker.main_module.exports,
            expected_exported_modules_keys
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_call_fn_esm() -> anyhow::Result<()> {
        let options = JsWorkerInitOptions {
            main_module_initializer: ModuleInitializer {
                mod_type: JsModuleType::Esm,
                url: ModuleSpecifier::from_file_path(
                    std::env::current_dir().unwrap().join("data/esm.js"),
                )
                .unwrap(),
            },
            node_modules_url: None,
        };
        let mut js_worker = JsWorker::init(options, None).await?;
        let arg = {
            let scope = &mut js_worker.main_worker.js_runtime.handle_scope();
            let arg: v8::Local<v8::Value> = v8::Integer::new(scope, 5).into();
            v8::Global::new(scope, arg)
        };

        let res1 = js_worker.call_fn("topFn", &[arg.clone()]).unwrap(); // sync fn without erroring
        let res2 = js_worker.call_async_fn("asyncFnResolve", &[arg]).await?; // async fn that will resolve
        let res3 = js_worker.call_fn("fnWithError", &[]); // sync fn that will error
        let res4 = js_worker.call_async_fn("asyncFnReject", &[]).await; // async fn that will reject

        let scope = &mut js_worker.main_worker.js_runtime.handle_scope();

        let res1 = v8::Local::new(scope, res1);
        let res1 = from_v8::<u32>(scope, res1)?;

        let res2 = v8::Local::new(scope, res2);
        let res2 = from_v8::<u32>(scope, res2)?;

        // res1 should return correct value ie 5 + 1
        assert_eq!(res1, 6);
        // res2 should return correct value ie 5 + 1
        assert_eq!(res2, 5);
        // res3 should error
        matches!(res3, Err(Error::JsException(_)));
        // res4 should error
        matches!(res4, Err(Error::DenoError(_)));

        Ok(())
    }

    #[tokio::test]
    async fn test_call_fn_cjs() -> anyhow::Result<()> {
        let options = JsWorkerInitOptions {
            main_module_initializer: ModuleInitializer {
                mod_type: JsModuleType::Cjs,
                url: ModuleSpecifier::from_file_path(
                    std::env::current_dir().unwrap().join("data/cjs.js"),
                )
                .unwrap(),
            },
            node_modules_url: None,
        };
        let mut js_worker = JsWorker::init(options, None).await?;
        let arg = {
            let scope = &mut js_worker.main_worker.js_runtime.handle_scope();
            let arg: v8::Local<v8::Value> = v8::Integer::new(scope, 5).into();
            v8::Global::new(scope, arg)
        };

        let res1 = js_worker.call_fn("topFn", &[arg.clone()]).unwrap(); // sync fn without erroring
        let res2 = js_worker.call_async_fn("asyncFnResolve", &[arg]).await?; // async fn that will resolve
        let res3 = js_worker.call_fn("fnWithError", &[]); // sync fn that will error
        let res4 = js_worker.call_async_fn("asyncFnReject", &[]).await; // async fn that will reject

        let scope = &mut js_worker.main_worker.js_runtime.handle_scope();

        let res1 = v8::Local::new(scope, res1);
        let res1 = from_v8::<u32>(scope, res1)?;

        let res2 = v8::Local::new(scope, res2);
        let res2 = from_v8::<u32>(scope, res2)?;

        // res1 should return correct value ie 5 + 1
        assert_eq!(res1, 6);
        // res2 should return correct value ie 5 + 1
        assert_eq!(res2, 5);
        // res3 should error
        matches!(res3, Err(Error::JsException(_)));
        // res4 should error
        matches!(res4, Err(Error::DenoError(_)));

        Ok(())
    }
}
