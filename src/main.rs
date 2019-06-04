use std::{cell::Cell, collections::HashMap, fs, io, path::Path};

use cargo_toml::Manifest;
use derivative::Derivative;
// use daggy::{Dag, NodeIndex, WouldCycle};

// 1. Read each `crate_type` into a list, where type is regular / dev.
// 2. For all `crate_type`s:
//
//     ```rust
//     if !links.contains(crate_type) {
//         let deps = read_manifest_deps();
//         links.insert(crate_type, deps);
//     }
//     ```
//
// 3. Print out graph.
//
// We don't care about version, because this is only what's in the current workspace.

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub enum DependencyType {
    Regular,
    Dev,
    Build,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, PartialOrd)]
pub enum State {
    NotProcessed,
    Processed,
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Dependency {
    /// Name of the crate.
    pub name: String,
    /// Type of dependency -- regular, dev, build.
    pub dep_type: DependencyType,
}

#[derive(Clone, Debug)]
pub struct CrateMetadata {
    /// Name of the crate.
    pub name: String,
    /// Cargo.toml manifest.
    pub manifest: Manifest,
}

#[derive(Clone, Debug, Derivative)]
#[derivative(Hash, PartialEq, Eq)]
pub struct Node {
    pub name: String,
    #[derivative(Hash = "ignore")]
    #[derivative(PartialEq = "ignore")]
    pub state: Cell<State>,
}

impl Node {
    pub fn mark_processed(&self) {
        self.state.set(State::Processed);
    }

    pub fn is_processed(&self) -> bool {
        self.state.get() == State::Processed
    }
}

fn read_crates<P>(dir: P) -> io::Result<Vec<CrateMetadata>>
where
    P: AsRef<Path>,
{
    let dir = dir.as_ref();
    dbg!(dir);
    let crate_metadatas = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let crate_dir = entry.path();
            let manifest_path = crate_dir.join("Cargo.toml");
            if manifest_path.exists() {
                Manifest::from_path(&manifest_path)
                    .ok()
                    .map(|manifest| (manifest_path, manifest))
            } else {
                None
            }
        })
        .map(|(manifest_path, manifest)| {
            let name = manifest
                .package
                .as_ref()
                .map(|package| package.name.clone())
                .unwrap_or_else(|| {
                    panic!(
                        "[package] section missing for manifest: {}",
                        manifest_path.display()
                    )
                });
            CrateMetadata { name, manifest }
        })
        .collect();

    Ok(crate_metadatas)
}

fn calculate_links(all_crates: Vec<CrateMetadata>) -> HashMap<Node, Vec<Dependency>> {
    all_crates
        .into_iter()
        .map(|crate_metadata| {
            let CrateMetadata { name, manifest } = crate_metadata;

            let node = Node {
                name,
                state: Cell::new(State::NotProcessed),
            };

            let dependencies_regular = manifest.dependencies;
            let dependencies_regular =
                dependencies_regular
                    .into_iter()
                    .map(|(name, _)| Dependency {
                        name,
                        dep_type: DependencyType::Regular,
                    });

            let dependencies_dev = manifest.dev_dependencies;
            let dependencies_dev = dependencies_dev.into_iter().map(|(name, _)| Dependency {
                name,
                dep_type: DependencyType::Dev,
            });

            let dependencies = dependencies_regular
                .chain(dependencies_dev)
                .collect::<Vec<Dependency>>();

            (node, dependencies)
        })
        .collect::<HashMap<Node, Vec<Dependency>>>()
}

fn _print_graph<'l>(links: &'l HashMap<Node, Vec<Dependency>>) {
    // While not all links have been visited, attach them to the tree.
    //
    // Build a graph. Start at a node:
    //
    // 1. Remove the node from links / mark it as seen.
    // 2. For each of its deps.
    //
    //     1. Print the child relationship to the current node.

    let node_style = r##"
        node [
            fillcolor = "#bbddff",
            fontname = "consolas",
            fontsize = 11,
            shape = box,
            style = filled,
            width = 1.5,
        ];
"##;
    println!("digraph World {{");
    println!("{}", node_style);
    println!("");

    links.iter().for_each(|(node, deps)| {
        if !node.is_processed() {
            node.mark_processed();
            deps.iter()
                // Only show workspace nodes
                .filter(|dep| links.keys().any(|node| node.name == dep.name))
                .for_each(|dep| {
                    if dep.dep_type == DependencyType::Dev {
                        println!("{} -> {}DEV;", &node.name, &dep.name);
                    } else {
                        println!("{} -> {};", &node.name, &dep.name);
                    }
                });
        }
    });

    println!("");
    println!("}}");
}

fn print_cycles<'l>(
    links: &'l HashMap<Node, Vec<Dependency>>,
    first_node: Option<&str>,
    is_subgraph: bool,
) -> Result<(), ()> {
    // TODO: while not all links have been visited, attach them to the tree.
    //
    // Build a graph. Start at a node:
    //
    // 1. Remove the node from links / mark it as seen.
    // 2. For each of its deps.
    //
    //     1. Register the child relationship to the current node.
    //
    // 3. For each of its unseen deps.
    //
    //     1. Recurse

    let links_iterator = links.iter();
    if let Some(first_node) = first_node {
        print_cycles_subgraph(
            links,
            links_iterator.skip_while(|(node, _deps)| node.name != first_node),
            is_subgraph,
        )
    } else {
        print_cycles_subgraph(links, links_iterator, is_subgraph)
    }
}

/// Returns true if the node was processed already (cycle detected), false otherwise.
fn print_cycles_subgraph<'l>(
    links: &'l HashMap<Node, Vec<Dependency>>,
    mut links_iterator: impl Iterator<Item = (&'l Node, &'l Vec<Dependency>)>,
    is_subgraph: bool,
) -> Result<(), ()> {
    links_iterator.try_for_each(|(node, deps)| {
        if !node.is_processed() {
            node.mark_processed();

            if !is_subgraph {
                println!("    subgraph cluster_{} {{", &node.name);
                println!("");
                println!("        style = dotted;");
                println!("");
            }

            let no_cycle = deps
                .iter()
                // Only show workspace nodes
                .filter(|dep| links.keys().any(|node| node.name == dep.name))
                .try_for_each(|dep| {
                    if dep.dep_type == DependencyType::Dev {
                        println!("{} -> {} [label = <<b>  DEV</b>>];", &node.name, &dep.name);
                    } else {
                        println!("{} -> {};", &node.name, &dep.name);
                    }

                    // Recurse
                    let result = print_cycles(links, Some(&dep.name), true);
                    if is_subgraph {
                        result
                    } else {
                        // if this is the first level, then just process all deps.
                        Ok(())
                    }
                });

            if !is_subgraph {
                println!("");
                println!("    }}");
            }

            if is_subgraph {
                no_cycle
            } else {
                Ok(())
            }
        } else {
            if is_subgraph {
                Err(())
            } else {
                Ok(())
            }
        }
    })
}

fn main() -> io::Result<()> {
    let mut all_crates = read_crates("app")?;
    all_crates.extend(read_crates("crate")?);

    let node_style = r##"
        node [
            fillcolor = "#bbddff",
            fontname = "consolas",
            fontsize = 11,
            shape = box,
            style = filled,
            width = 1.5,
        ];
"##;
    println!("digraph World {{");
    println!("{}", node_style);
    println!("");

    let links = calculate_links(all_crates);

    let _ = print_cycles(&links, Some("will"), false);

    println!("");
    println!("}}");

    Ok(())
}
