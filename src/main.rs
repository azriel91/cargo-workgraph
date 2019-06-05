use std::{cell::Cell, cmp::Ordering, collections::HashSet, fs, io, path::Path};

use cargo_toml::Manifest;
use derivative::Derivative;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd)]
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

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd)]
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
    #[derivative(Hash = "ignore", PartialEq = "ignore")]
    pub dependencies: HashSet<Dependency>,
    #[derivative(Hash = "ignore", PartialEq = "ignore")]
    pub state: Cell<State>,
}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Node) -> Option<Ordering> {
        Some(self.name.cmp(&other.name))
    }
}

impl Ord for Node {
    fn cmp(&self, other: &Node) -> Ordering {
        self.name.cmp(&other.name)
    }
}

impl Node {
    pub fn mark_processed(&self) {
        self.state.set(State::Processed);
    }

    pub fn is_processed(&self) -> bool {
        self.state.get() == State::Processed
    }
}

/// A cycle is a chain of crates that end up in a dependency circle
#[derive(Debug, Hash, Eq)]
pub struct Cycle(pub Vec<Node>);

impl PartialEq for Cycle {
    fn eq(&self, other: &Self) -> bool {
        self.0.clone().sort() == other.0.clone().sort()
    }
}

fn read_crates<P>(dir: P) -> io::Result<Vec<CrateMetadata>>
where
    P: AsRef<Path>,
{
    let dir = dir.as_ref();
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

fn build_nodes(all_crates: Vec<CrateMetadata>) -> Vec<Node> {
    let crate_names = all_crates
        .iter()
        .map(|crate_metadata| crate_metadata.name.clone())
        .collect::<Vec<String>>();

    all_crates
        .into_iter()
        .map(|crate_metadata| {
            let CrateMetadata { name, manifest } = crate_metadata;

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
                .filter(|dep| crate_names.iter().any(|name| name == &dep.name))
                .collect::<HashSet<Dependency>>();

            Node {
                name,
                dependencies,
                state: Cell::new(State::NotProcessed),
            }
        })
        .collect::<Vec<Node>>()
}

fn detect_cycles_all<'l>(nodes: &'l Vec<Node>) -> HashSet<Cycle> {
    // Clone while no nodes are marked processed.
    nodes
        .iter()
        .flat_map(|node| detect_cycle(&(nodes.clone()), &mut Vec::new(), node))
        .collect::<HashSet<Cycle>>()
}

fn detect_cycle<'n>(nodes: &'n [Node], node_buffer: &mut Vec<Node>, node: &'n Node) -> Vec<Cycle> {
    if node.is_processed() && node_buffer.contains(node) {
        // Found a cycle
        // let node_cycle_start = node;

        // Delete all the nodes in the cycle buffer before cycle_start.
        let cycle = Cycle(
            node_buffer
                .drain(..)
                // .skip_while(|node| node != node_cycle_start)
                .collect::<Vec<Node>>(),
        );

        vec![cycle]
    } else {
        node.mark_processed();

        let filter: fn(&&Dependency) -> bool = if node_buffer.is_empty() {
            |_dep: &&Dependency| true
        } else {
            |dep: &&Dependency| dep.dep_type != DependencyType::Dev
        };

        node_buffer.push(node.clone());

        // Detect the first one.
        node.dependencies
            .iter()
            .filter(filter)
            .fold(Vec::new(), |mut cycles, dep| {
                let dep_node = nodes
                    .iter()
                    .find(|node| &node.name == &dep.name)
                    .unwrap_or_else(|| {
                        panic!(
                            "Expected `{}` to have dependency on: `{}`",
                            &node.name, &dep.name
                        )
                    });

                cycles.extend(detect_cycle(nodes, &mut (node_buffer.clone()), dep_node));
                cycles
            })
    }
}

fn print_cycles(cycles: HashSet<Cycle>) {
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

    cycles.iter().enumerate().for_each(|(index, cycle)| {
        let nodes = &cycle.0;

        // TODO: Why do we get single node cycles?
        if nodes.len() >= 2 {
            println!("    subgraph cluster_{} {{", index);
            println!("");
            println!("        style = dotted;");
            println!("");

            nodes.iter().for_each(|node| {
                println!(
                    "        {krate}{index} [label = {krate}];",
                    krate = &node.name,
                    index = index
                );
            });

            println!("");

            (0..(nodes.len() - 1)).for_each(|i| {
                let node = &nodes[i];
                let neighbour = {
                    let neighbour_index = if i == nodes.len() - 1 { 0 } else { i + 1 };
                    &nodes[neighbour_index]
                };

                let dep = node
                    .dependencies
                    .iter()
                    .find(|dep| &dep.name == &neighbour.name);
                // .unwrap_or_else(|| {
                //     panic!(
                //         "Expected `{}` to have dependency on: `{}`",
                //         &node.name, &neighbour.name
                //     )
                // });

                if let Some(dep) = dep.as_ref() {
                    if dep.dep_type == DependencyType::Dev {
                        println!(
                            "{krate}{index} -> {dep}{index} [label = <<b>  DEV</b>>];",
                            krate = &node.name,
                            dep = &dep.name,
                            index = index
                        );
                    } else {
                        println!(
                            "{krate}{index} -> {dep}{index} [label = <<b>  REG</b>>];",
                            krate = &node.name,
                            dep = &dep.name,
                            index = index
                        );
                    }
                } else {
                    // println!(
                    //     "{krate}{index} -> {dep}{index};",
                    //     krate = &node.name,
                    //     dep = &neighbour.name,
                    //     index = index
                    // );
                }
            });

            println!("");
            println!("    }}");
        }
    });

    println!("");
    println!("}}");
}

fn main() -> io::Result<()> {
    let mut all_crates = read_crates("app")?;
    all_crates.extend(read_crates("crate")?);

    let nodes = build_nodes(all_crates);
    let cycles = detect_cycles_all(&nodes);

    print_cycles(cycles);

    Ok(())
}
