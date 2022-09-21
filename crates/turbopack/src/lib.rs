#![feature(box_patterns)]
#![feature(box_syntax)]
#![feature(trivial_bounds)]
#![feature(min_specialization)]
#![feature(map_try_insert)]
#![feature(option_get_or_insert_default)]
#![feature(once_cell)]
#![feature(hash_set_entry)]
#![recursion_limit = "256"]

use std::{
    collections::{HashMap, HashSet},
    mem::swap,
};

use anyhow::Result;
use ecmascript::typescript::resolve::TypescriptTypesAssetReferenceVc;
use graph::{aggregate, AggregatedGraphNodeContent, AggregatedGraphVc};
use module_options::{
    ModuleOptionsContextVc, ModuleOptionsVc, ModuleRuleEffect, ModuleRuleEffectKey, ModuleType,
};
pub use resolve::resolve_options;
use turbo_tasks::{primitives::BoolVc, CompletionVc, Value};
use turbo_tasks_fs::FileSystemPathVc;
use turbopack_core::{
    asset::AssetVc,
    context::{AssetContext, AssetContextVc},
    environment::EnvironmentVc,
    reference::all_referenced_assets,
    resolve::{options::ResolveOptionsVc, parse::RequestVc, resolve, ResolveResultVc},
};

mod graph;
pub mod json;
pub mod module_options;
pub mod rebase;
pub mod resolve;
pub mod resolve_options_context;
pub mod transition;

pub use turbopack_css as css;
pub use turbopack_ecmascript as ecmascript;

use self::{
    resolve_options_context::ResolveOptionsContextVc,
    transition::{TransitionVc, TransitionsByNameVc},
};

#[turbo_tasks::function]
async fn module(source: AssetVc, context: ModuleAssetContextVc) -> Result<AssetVc> {
    let path = source.path();
    let options = ModuleOptionsVc::new(path.parent(), context.module_options_context());
    let options = options.await?;
    let path_value = path.await?;

    let mut effects = HashMap::new();
    for rule in options.rules.iter() {
        if rule.matches(&path_value) {
            effects.extend(rule.effects());
        }
    }

    Ok(
        match effects
            .get(&ModuleRuleEffectKey::ModuleType)
            .map(|e| {
                if let ModuleRuleEffect::ModuleType(ty) = e {
                    ty
                } else {
                    unreachable!()
                }
            })
            .unwrap_or_else(|| &ModuleType::Raw)
        {
            ModuleType::Ecmascript(transforms) => {
                turbopack_ecmascript::EcmascriptModuleAssetVc::new(
                    source,
                    context.into(),
                    Value::new(turbopack_ecmascript::ModuleAssetType::Ecmascript),
                    *transforms,
                    context.environment(),
                )
                .into()
            }
            ModuleType::Typescript(transforms) => {
                turbopack_ecmascript::EcmascriptModuleAssetVc::new(
                    source,
                    context.with_typescript_resolving_enabled().into(),
                    Value::new(turbopack_ecmascript::ModuleAssetType::Typescript),
                    *transforms,
                    context.environment(),
                )
                .into()
            }
            ModuleType::TypescriptDeclaration(transforms) => {
                turbopack_ecmascript::EcmascriptModuleAssetVc::new(
                    source,
                    context.with_typescript_resolving_enabled().into(),
                    Value::new(turbopack_ecmascript::ModuleAssetType::TypescriptDeclaration),
                    *transforms,
                    context.environment(),
                )
                .into()
            }
            ModuleType::Json => json::JsonModuleAssetVc::new(source).into(),
            ModuleType::Raw => source,
            ModuleType::Css => turbopack_css::CssModuleAssetVc::new(source, context.into()).into(),
            ModuleType::Static => {
                turbopack_static::StaticModuleAssetVc::new(source, context.into()).into()
            }
            ModuleType::Custom(_) => todo!(),
        },
    )
}

#[turbo_tasks::value]
pub struct ModuleAssetContext {
    transitions: TransitionsByNameVc,
    context_path: FileSystemPathVc,
    environment: EnvironmentVc,
    module_options_context: ModuleOptionsContextVc,
    resolve_options_context: ResolveOptionsContextVc,
    transition: Option<TransitionVc>,
}

#[turbo_tasks::value_impl]
impl ModuleAssetContextVc {
    #[turbo_tasks::function]
    pub fn new(
        transitions: TransitionsByNameVc,
        context_path: FileSystemPathVc,
        environment: EnvironmentVc,
        module_options_context: ModuleOptionsContextVc,
        resolve_options_context: ResolveOptionsContextVc,
    ) -> Self {
        Self::cell(ModuleAssetContext {
            transitions,
            context_path,
            environment,
            module_options_context,
            resolve_options_context,
            transition: None,
        })
    }

    #[turbo_tasks::function]
    pub fn new_transition(
        transitions: TransitionsByNameVc,
        context_path: FileSystemPathVc,
        environment: EnvironmentVc,
        module_options_context: ModuleOptionsContextVc,
        resolve_options_context: ResolveOptionsContextVc,
        transition: TransitionVc,
    ) -> Self {
        Self::cell(ModuleAssetContext {
            transitions,
            context_path,
            environment,
            module_options_context,
            resolve_options_context,
            transition: Some(transition),
        })
    }

    #[turbo_tasks::function]
    pub async fn module_options_context(self) -> Result<ModuleOptionsContextVc> {
        Ok(self.await?.module_options_context)
    }

    #[turbo_tasks::function]
    pub async fn is_typescript_resolving_enabled(self) -> Result<BoolVc> {
        Ok(BoolVc::cell(
            self.await?.resolve_options_context.await?.enable_typescript,
        ))
    }

    #[turbo_tasks::function]
    pub async fn with_typescript_resolving_enabled(self) -> Result<ModuleAssetContextVc> {
        if *self.is_typescript_resolving_enabled().await? {
            return Ok(self);
        }
        let this = self.await?;
        let resolve_options_context = this
            .resolve_options_context
            .with_typescript_enabled()
            .resolve()
            .await?;
        Ok(ModuleAssetContextVc::new(
            this.transitions,
            this.context_path,
            this.environment,
            this.module_options_context,
            resolve_options_context,
        ))
    }
}

#[turbo_tasks::value_impl]
impl AssetContext for ModuleAssetContext {
    #[turbo_tasks::function]
    fn context_path(&self) -> FileSystemPathVc {
        self.context_path
    }

    #[turbo_tasks::function]
    fn environment(&self) -> EnvironmentVc {
        self.environment
    }

    #[turbo_tasks::function]
    async fn resolve_options(&self) -> Result<ResolveOptionsVc> {
        Ok(resolve_options(
            self.context_path,
            self.resolve_options_context,
        ))
    }

    #[turbo_tasks::function]
    async fn resolve_asset(
        self_vc: ModuleAssetContextVc,
        context_path: FileSystemPathVc,
        request: RequestVc,
        resolve_options: ResolveOptionsVc,
    ) -> Result<ResolveResultVc> {
        let this = self_vc.await?;

        let result = resolve(context_path, request, resolve_options);
        let result = self_vc.process_resolve_result(result);

        if *self_vc.is_typescript_resolving_enabled().await? {
            let types_reference = TypescriptTypesAssetReferenceVc::new(
                ModuleAssetContextVc::new(
                    this.transitions,
                    context_path,
                    this.environment,
                    this.module_options_context,
                    this.resolve_options_context,
                )
                .into(),
                request,
            );

            result.add_reference(types_reference.into());
        }

        Ok(result)
    }

    #[turbo_tasks::function]
    async fn process_resolve_result(
        self_vc: ModuleAssetContextVc,
        result: ResolveResultVc,
    ) -> Result<ResolveResultVc> {
        Ok(result
            .await?
            .map(|a| self_vc.process(a).resolve(), |i| async move { Ok(i) })
            .await?
            .into())
    }

    #[turbo_tasks::function]
    async fn process(self_vc: ModuleAssetContextVc, asset: AssetVc) -> Result<AssetVc> {
        let this = self_vc.await?;
        if let Some(transition) = this.transition {
            let asset = transition.process_source(asset);
            let environment = transition.process_environment(this.environment);
            let module_options_context =
                transition.process_module_options_context(this.module_options_context);
            let resolve_options_context =
                transition.process_resolve_options_context(this.resolve_options_context);
            let context = ModuleAssetContextVc::new(
                this.transitions,
                asset.path().parent(),
                environment,
                module_options_context,
                resolve_options_context,
            );
            let m = module(asset, context);
            Ok(transition.process_module(m, context))
        } else {
            let context = ModuleAssetContextVc::new(
                this.transitions,
                asset.path().parent(),
                this.environment,
                this.module_options_context,
                this.resolve_options_context,
            );
            Ok(module(asset, context))
        }
    }

    #[turbo_tasks::function]
    fn with_context_path(&self, path: FileSystemPathVc) -> AssetContextVc {
        ModuleAssetContextVc::new(
            self.transitions,
            path,
            self.environment,
            self.module_options_context,
            self.resolve_options_context,
        )
        .into()
    }

    #[turbo_tasks::function]
    fn with_environment(&self, environment: EnvironmentVc) -> AssetContextVc {
        ModuleAssetContextVc::new(
            self.transitions,
            self.context_path,
            environment,
            self.module_options_context,
            self.resolve_options_context,
        )
        .into()
    }

    #[turbo_tasks::function]
    async fn with_transition(&self, transition: &str) -> Result<AssetContextVc> {
        Ok(
            if let Some(transition) = self.transitions.await?.get(transition) {
                ModuleAssetContextVc::new_transition(
                    self.transitions,
                    self.context_path,
                    self.environment,
                    self.module_options_context,
                    self.resolve_options_context,
                    *transition,
                )
                .into()
            } else {
                // TODO report issue
                ModuleAssetContextVc::new(
                    self.transitions,
                    self.context_path,
                    self.environment,
                    self.module_options_context,
                    self.resolve_options_context,
                )
                .into()
            },
        )
    }
}

#[turbo_tasks::function]
pub async fn emit(asset: AssetVc) {
    emit_assets_recursive(asset);
}

#[turbo_tasks::function]
pub async fn emit_with_completion(asset: AssetVc, output_dir: FileSystemPathVc) -> CompletionVc {
    emit_assets_aggregated(asset, output_dir)
}

#[turbo_tasks::function]
async fn emit_assets_aggregated(asset: AssetVc, output_dir: FileSystemPathVc) -> CompletionVc {
    let aggregated = aggregate(asset);
    emit_aggregated_assets(aggregated, output_dir)
}

#[turbo_tasks::function]
async fn emit_aggregated_assets(
    aggregated: AggregatedGraphVc,
    output_dir: FileSystemPathVc,
) -> Result<CompletionVc> {
    Ok(match &*aggregated.content().await? {
        AggregatedGraphNodeContent::Asset(asset) => emit_asset_into_dir(*asset, output_dir),
        AggregatedGraphNodeContent::Children(children) => {
            for aggregated in children {
                emit_aggregated_assets(*aggregated, output_dir).await?;
            }
            CompletionVc::new()
        }
    })
}

#[turbo_tasks::function(cycle)]
async fn emit_assets_recursive(asset: AssetVc) -> Result<()> {
    let assets_set = all_referenced_assets(asset);
    emit_asset(asset);
    for asset in assets_set.await?.iter() {
        emit_assets_recursive(*asset);
    }
    Ok(())
}

#[turbo_tasks::function]
pub fn emit_asset(asset: AssetVc) -> CompletionVc {
    asset.path().write(asset.content())
}

#[turbo_tasks::function]
pub async fn emit_asset_into_dir(
    asset: AssetVc,
    output_dir: FileSystemPathVc,
) -> Result<CompletionVc> {
    let dir = &*output_dir.await?;
    Ok(if asset.path().await?.is_inside(dir) {
        asset.path().write(asset.content())
    } else {
        CompletionVc::new()
    })
}

#[turbo_tasks::function]
pub fn print_most_referenced(asset: AssetVc) {
    let aggregated = aggregate(asset);
    let back_references = compute_back_references(aggregated);
    let sorted_back_references = top_references(back_references);
    print_references(sorted_back_references);
}

#[turbo_tasks::value(shared)]
struct ReferencesList {
    referenced_by: HashMap<AssetVc, HashSet<AssetVc>>,
}

#[turbo_tasks::function]
async fn compute_back_references(aggregated: AggregatedGraphVc) -> Result<ReferencesListVc> {
    Ok(match &*aggregated.content().await? {
        AggregatedGraphNodeContent::Asset(asset) => {
            let mut referenced_by = HashMap::new();
            for reference in all_referenced_assets(*asset).await?.iter() {
                referenced_by.insert(*reference, [*asset].into_iter().collect());
            }
            ReferencesList { referenced_by }.into()
        }
        AggregatedGraphNodeContent::Children(children) => {
            let mut referenced_by = HashMap::<AssetVc, HashSet<AssetVc>>::new();
            let lists = children
                .iter()
                .map(|child| compute_back_references(*child))
                .collect::<Vec<_>>();
            for list in lists {
                for (key, values) in list.await?.referenced_by.iter() {
                    if let Some(set) = referenced_by.get_mut(key) {
                        for value in values {
                            set.insert(*value);
                        }
                    } else {
                        referenced_by.insert(*key, values.clone());
                    }
                }
            }
            ReferencesList { referenced_by }.into()
        }
    })
}

#[turbo_tasks::function]
async fn top_references(list: ReferencesListVc) -> Result<ReferencesListVc> {
    let list = list.await?;
    const N: usize = 5;
    let mut top = Vec::<(&AssetVc, &HashSet<AssetVc>)>::new();
    for tuple in list.referenced_by.iter() {
        let mut current = tuple;
        for item in &mut top {
            if item.1.len() < tuple.1.len() {
                swap(item, &mut current);
            }
        }
        if top.len() < N {
            top.push(current);
        }
    }
    Ok(ReferencesList {
        referenced_by: top
            .into_iter()
            .map(|(asset, set)| (*asset, set.clone()))
            .collect(),
    }
    .into())
}

#[turbo_tasks::function]
async fn print_references(list: ReferencesListVc) -> Result<()> {
    let list = list.await?;
    println!("TOP REFERENCES:");
    for (asset, references) in list.referenced_by.iter() {
        println!(
            "{} -> {} times referenced",
            asset.path().await?.path,
            references.len()
        );
    }
    Ok(())
}

pub fn register() {
    turbo_tasks::register();
    turbo_tasks_fs::register();
    turbopack_core::register();
    turbopack_ecmascript::register();
    turbopack_css::register();
    turbopack_static::register();
    include!(concat!(env!("OUT_DIR"), "/register.rs"));
}