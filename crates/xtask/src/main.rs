use std::{
    collections::{HashMap, HashSet},
    env::{current_dir, var_os},
    path::PathBuf,
    process,
};

use clap::{arg, Command};

mod command;
mod nft_bench;
mod publish;
mod summarize_bench;
mod visualize_bundler_bench;

use nft_bench::show_result;
use publish::{publish_workspace, run_bump, run_publish};

fn cli() -> Command<'static> {
    Command::new("xtask")
        .about("turbo-tooling cargo tasks")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .allow_external_subcommands(true)
        .allow_invalid_utf8_for_external_subcommands(true)
        .subcommand(
            Command::new("npm")
                .about("Publish binaries to npm")
                .arg(arg!(<NAME> "the package to publish"))
                .arg_required_else_help(true),
        )
        .subcommand(
            Command::new("workspace")
                .arg(arg!(--publish "publish npm packages in yarn workspace"))
                .arg(arg!(--bump "bump new version for npm package in yarn workspace"))
                .arg(arg!(--"dry-run" "dry run all operations"))
                .arg(arg!([NAME] "the package to bump"))
                .about("Manage packages in yarn workspaces"),
        )
        .subcommand(
            Command::new("nft-bench-result")
                .about("Print node-file-trace benchmark result against @vercel/nft"),
        )
        .subcommand(
            Command::new("upgrade-swc")
                .about("Upgrade all SWC dependencies to the lastest version"),
        )
        .subcommand(
            Command::new("summarize-benchmarks")
                .about(
                    "Normalize all raw data based on similar benchmarks, average data by \
                     system+sha and compute latest by system",
                )
                .arg(arg!(<PATH> "the path to the benchmark data directory")),
        )
        .subcommand(
            Command::new("visualize-bundler-benchmarks")
                .about("Generate visualizations of bundler benchmarks")
                .arg(arg!(<PATH> "the path to the benchmark data directory")),
        )
}

fn main() {
    let matches = cli().get_matches();
    match matches.subcommand() {
        Some(("npm", sub_matches)) => {
            let name = sub_matches
                .get_one::<String>("NAME")
                .expect("NAME is required");
            run_publish(name);
        }
        Some(("workspace", sub_matches)) => {
            let is_bump = sub_matches.is_present("bump");
            let is_publish = sub_matches.is_present("publish");
            let dry_run = sub_matches.is_present("dry-run");
            if is_bump {
                let names = sub_matches
                    .get_many::<String>("NAME")
                    .map(|names| names.cloned().collect::<HashSet<_>>())
                    .unwrap_or_default();
                run_bump(names, dry_run);
            }
            if is_publish {
                publish_workspace(dry_run);
            }
        }
        Some(("nft-bench-result", _)) => {
            show_result();
        }
        Some(("upgrade-swc", _)) => {
            let workspace_dir = var_os("CARGO_WORKSPACE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| current_dir().unwrap());
            let cargo_lock_path = workspace_dir.join("Cargo.lock");
            let lock = cargo_lock::Lockfile::load(cargo_lock_path).unwrap();
            let swc_packages = lock
                .packages
                .iter()
                .filter(|p| {
                    p.name.as_str().starts_with("swc_")
                        || p.name.as_str() == "swc"
                        || p.name.as_str() == "testing"
                })
                .collect::<Vec<_>>();
            let only_swc_set = swc_packages
                .iter()
                .map(|p| p.name.as_str())
                .collect::<HashSet<_>>();
            let packages = lock
                .packages
                .iter()
                .map(|p| (format!("{}@{}", p.name, p.version), p))
                .collect::<HashMap<_, _>>();
            let mut queue = swc_packages.clone();
            let mut set = HashSet::new();
            while let Some(package) = queue.pop() {
                for dep in package.dependencies.iter() {
                    let ident = format!("{}@{}", dep.name, dep.version);
                    let package = *packages.get(&ident).unwrap();
                    if set.insert(ident) {
                        queue.push(package);
                    }
                }
            }
            let status = process::Command::new("cargo")
                .arg("upgrade")
                .arg("--workspace")
                .args(only_swc_set.into_iter())
                .current_dir(&workspace_dir)
                .stdout(process::Stdio::inherit())
                .stderr(process::Stdio::inherit())
                .status()
                .expect("Running cargo upgrade failed");
            assert!(status.success());
            let status = process::Command::new("cargo")
                .arg("update")
                .args(set.iter().flat_map(|p| ["-p", p]))
                .current_dir(&workspace_dir)
                .stdout(process::Stdio::inherit())
                .stderr(process::Stdio::inherit())
                .status()
                .expect("Running cargo update failed");
            assert!(status.success());
        }
        Some(("summarize-benchmarks", sub_matches)) => {
            let path = sub_matches
                .get_one::<String>("PATH")
                .expect("PATH is required");
            let path = PathBuf::from(path);
            let path = path.canonicalize().unwrap();
            summarize_bench::process_all(path);
        }
        Some(("visualize-bundler-benchmarks", sub_matches)) => {
            let path = sub_matches
                .get_one::<String>("PATH")
                .expect("PATH is required");
            let path = PathBuf::from(path);
            let path = path.canonicalize().unwrap();
            visualize_bundler_bench::generate(path).unwrap();
        }
        _ => {
            panic!("Unknown command {:?}", matches.subcommand().map(|c| c.0));
        }
    }
}