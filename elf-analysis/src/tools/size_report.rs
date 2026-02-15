use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{debug, info};

use super::types::*;

/// Run the Zephyr size_report script on an ELF file.
/// Returns a map of target name ("rom", "ram") to the generated JSON file path.
pub async fn run_size_report(
    elf_path: &Path,
    zephyr_base: &Path,
    workspace: Option<&Path>,
    targets: &[&str],
) -> Result<HashMap<String, PathBuf>, String> {
    let script = zephyr_base.join("scripts/footprint/size_report");
    if !script.exists() {
        return Err(format!(
            "size_report script not found at {}. Set --zephyr-base or --workspace.",
            script.display()
        ));
    }

    let tmpdir = tempfile::tempdir()
        .map_err(|e| format!("Failed to create temp dir: {}", e))?;

    let mut cmd_args = vec![
        script.to_string_lossy().to_string(),
        "-k".to_string(),
        elf_path.to_string_lossy().to_string(),
        "-z".to_string(),
        zephyr_base.to_string_lossy().to_string(),
        "-o".to_string(),
        tmpdir.path().to_string_lossy().to_string(),
        "-q".to_string(),
    ];

    if let Some(ws) = workspace {
        cmd_args.push("-w".to_string());
        cmd_args.push(ws.to_string_lossy().to_string());
    }

    for t in targets {
        cmd_args.push(t.to_string());
    }

    info!("Running: python3 {}", cmd_args.join(" "));

    let output = Command::new("python3")
        .args(&cmd_args)
        .output()
        .await
        .map_err(|e| format!("Failed to execute python3: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("ImportError") || stderr.contains("ModuleNotFoundError") {
            return Err(format!(
                "Missing Python dependencies. Install with: pip install pyelftools anytree colorama packaging\n\nFull error:\n{}",
                stderr
            ));
        }
        return Err(format!("size_report failed:\n{}", stderr));
    }

    // Collect the generated JSON files
    let mut results = HashMap::new();
    for t in targets {
        let json_path = tmpdir.path().join(format!("{}.json", t));
        if json_path.exists() {
            results.insert(t.to_string(), json_path);
        }
    }

    // Persist the tmpdir so files remain accessible
    // Caller must use the files before they go out of scope
    std::mem::forget(tmpdir);

    debug!("size_report produced {} JSON files", results.len());
    Ok(results)
}

/// Parse a size_report JSON file into our SizeReport type.
pub fn parse_size_json(path: &Path) -> Result<SizeReport, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    parse_size_json_str(&content)
}

/// Parse size_report JSON from a string (useful for testing).
pub fn parse_size_json_str(content: &str) -> Result<SizeReport, String> {
    let raw: SizeReportJson = serde_json::from_str(content)
        .map_err(|e| format!("Failed to parse size_report JSON: {}", e))?;

    let tree = convert_node(&raw.symbols);
    let used_size = sum_children_size(&tree);

    Ok(SizeReport {
        total_size: raw.total_size,
        used_size,
        tree,
    })
}

fn convert_node(node: &SizeReportNode) -> SizeNode {
    SizeNode {
        name: node.identifier.clone(),
        size: node.size,
        children: node.children.iter().map(convert_node).collect(),
    }
}

/// Sum sizes of all direct children (leaf sizes bubble up).
fn sum_children_size(node: &SizeNode) -> u64 {
    if node.children.is_empty() {
        node.size
    } else {
        node.children.iter().map(sum_children_size).sum()
    }
}

/// Truncate tree to a maximum depth. Nodes beyond the depth limit
/// are collapsed: their sizes are summed into the parent.
pub fn truncate_tree(node: &SizeNode, max_depth: u32) -> SizeNode {
    truncate_tree_inner(node, max_depth, 0)
}

fn truncate_tree_inner(node: &SizeNode, max_depth: u32, current_depth: u32) -> SizeNode {
    if current_depth >= max_depth || node.children.is_empty() {
        // Collapse all children into this node
        SizeNode {
            name: node.name.clone(),
            size: node.size,
            children: vec![],
        }
    } else {
        SizeNode {
            name: node.name.clone(),
            size: node.size,
            children: node.children.iter()
                .map(|c| truncate_tree_inner(c, max_depth, current_depth + 1))
                .collect(),
        }
    }
}

/// Flatten tree to collect nodes at a given level.
/// "file" level = nodes whose children are all leaves (symbols).
/// "symbol" level = leaf nodes only.
pub fn flatten_tree(node: &SizeNode, level: &str) -> Vec<Consumer> {
    let mut items = Vec::new();
    let total = if node.size > 0 { node.size } else { 1 }; // avoid div by zero

    match level {
        "symbol" => collect_leaves(node, &mut items),
        _ => collect_files(node, &mut items), // "file" is default
    }

    // Calculate percentages
    let items: Vec<Consumer> = items.into_iter()
        .map(|(path, size)| Consumer {
            path,
            size,
            percent: (size as f64 / total as f64) * 100.0,
        })
        .collect();

    // Sort descending by size
    let mut items = items;
    items.sort_by(|a, b| b.size.cmp(&a.size));
    items
}

/// Collect leaf nodes (symbols).
fn collect_leaves(node: &SizeNode, out: &mut Vec<(String, u64)>) {
    if node.children.is_empty() {
        if node.size > 0 {
            out.push((node.name.clone(), node.size));
        }
    } else {
        for child in &node.children {
            collect_leaves(child, out);
        }
    }
}

/// Collect "file" level nodes — nodes whose children are all leaves.
fn collect_files(node: &SizeNode, out: &mut Vec<(String, u64)>) {
    if node.children.is_empty() {
        // This is a leaf — it's both a symbol and a "file" if standalone
        if node.size > 0 {
            out.push((node.name.clone(), node.size));
        }
    } else {
        let all_children_are_leaves = node.children.iter().all(|c| c.children.is_empty());
        if all_children_are_leaves {
            // This node is a "file" — sum its children
            out.push((node.name.clone(), node.size));
        } else {
            for child in &node.children {
                collect_files(child, out);
            }
        }
    }
}

/// Diff two size report trees and return top increases and decreases.
pub fn diff_trees(a: &SizeReport, b: &SizeReport, limit: usize) -> (Vec<NodeDelta>, Vec<NodeDelta>) {
    // Flatten both trees to file level for comparison
    let a_files = flatten_to_map(&a.tree);
    let b_files = flatten_to_map(&b.tree);

    // Compute deltas for all paths present in either tree
    let mut all_paths: Vec<&String> = a_files.keys().chain(b_files.keys()).collect();
    all_paths.sort();
    all_paths.dedup();

    let mut deltas: Vec<NodeDelta> = all_paths.iter().map(|path| {
        let before = a_files.get(*path).copied().unwrap_or(0);
        let after = b_files.get(*path).copied().unwrap_or(0);
        NodeDelta {
            path: path.to_string(),
            before,
            after,
            delta: after as i64 - before as i64,
        }
    })
    .filter(|d| d.delta != 0)
    .collect();

    // Sort by absolute delta descending
    deltas.sort_by(|a, b| b.delta.abs().cmp(&a.delta.abs()));

    let increases: Vec<NodeDelta> = deltas.iter()
        .filter(|d| d.delta > 0)
        .take(limit)
        .map(|d| NodeDelta {
            path: d.path.clone(),
            before: d.before,
            after: d.after,
            delta: d.delta,
        })
        .collect();

    let decreases: Vec<NodeDelta> = deltas.iter()
        .filter(|d| d.delta < 0)
        .take(limit)
        .map(|d| NodeDelta {
            path: d.path.clone(),
            before: d.before,
            after: d.after,
            delta: d.delta,
        })
        .collect();

    (increases, decreases)
}

/// Flatten a tree into a map of path -> size (leaf level for diffing).
fn flatten_to_map(node: &SizeNode) -> HashMap<String, u64> {
    let mut map = HashMap::new();
    flatten_to_map_inner(node, &mut map);
    map
}

fn flatten_to_map_inner(node: &SizeNode, map: &mut HashMap<String, u64>) {
    if node.children.is_empty() {
        if node.size > 0 {
            *map.entry(node.name.clone()).or_insert(0) += node.size;
        }
    } else {
        for child in &node.children {
            flatten_to_map_inner(child, map);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tree() -> SizeNode {
        SizeNode {
            name: "root".to_string(),
            size: 1000,
            children: vec![
                SizeNode {
                    name: "zephyr/kernel/sched.c".to_string(),
                    size: 500,
                    children: vec![
                        SizeNode {
                            name: "zephyr/kernel/sched.c/k_sched_lock".to_string(),
                            size: 200,
                            children: vec![],
                        },
                        SizeNode {
                            name: "zephyr/kernel/sched.c/k_sched_unlock".to_string(),
                            size: 300,
                            children: vec![],
                        },
                    ],
                },
                SizeNode {
                    name: "zephyr/kernel/thread.c".to_string(),
                    size: 400,
                    children: vec![
                        SizeNode {
                            name: "zephyr/kernel/thread.c/k_thread_create".to_string(),
                            size: 400,
                            children: vec![],
                        },
                    ],
                },
                SizeNode {
                    name: "app/main.c".to_string(),
                    size: 100,
                    children: vec![],
                },
            ],
        }
    }

    fn sample_json() -> &'static str {
        r#"{
            "symbols": {
                "identifier": "root",
                "name": "Root",
                "size": 1000,
                "children": [
                    {
                        "identifier": "zephyr/kernel/sched.c",
                        "name": "sched.c",
                        "size": 500,
                        "children": [
                            {
                                "identifier": "zephyr/kernel/sched.c/k_sched_lock",
                                "name": "k_sched_lock",
                                "size": 200,
                                "children": []
                            },
                            {
                                "identifier": "zephyr/kernel/sched.c/k_sched_unlock",
                                "name": "k_sched_unlock",
                                "size": 300,
                                "children": []
                            }
                        ]
                    },
                    {
                        "identifier": "app/main.c",
                        "name": "main.c",
                        "size": 300,
                        "children": []
                    }
                ]
            },
            "total_size": 1000
        }"#
    }

    #[test]
    fn test_parse_size_json() {
        let report = parse_size_json_str(sample_json()).unwrap();
        assert_eq!(report.total_size, 1000);
        assert_eq!(report.used_size, 800); // 200 + 300 + 300
        assert_eq!(report.tree.name, "root");
        assert_eq!(report.tree.children.len(), 2);
    }

    #[test]
    fn test_parse_size_json_invalid() {
        let result = parse_size_json_str("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_truncate_tree_depth_0() {
        let tree = sample_tree();
        let truncated = truncate_tree(&tree, 0);
        assert_eq!(truncated.name, "root");
        assert!(truncated.children.is_empty());
        assert_eq!(truncated.size, 1000);
    }

    #[test]
    fn test_truncate_tree_depth_1() {
        let tree = sample_tree();
        let truncated = truncate_tree(&tree, 1);
        assert_eq!(truncated.children.len(), 3);
        // All children should have no grandchildren
        for child in &truncated.children {
            assert!(child.children.is_empty());
        }
    }

    #[test]
    fn test_truncate_tree_depth_unlimited() {
        let tree = sample_tree();
        let truncated = truncate_tree(&tree, 100);
        // Should preserve full structure
        assert_eq!(truncated.children[0].children.len(), 2);
    }

    #[test]
    fn test_flatten_tree_file_level() {
        let tree = sample_tree();
        let consumers = flatten_tree(&tree, "file");
        // Should get sched.c, thread.c, main.c
        assert_eq!(consumers.len(), 3);
        assert_eq!(consumers[0].path, "zephyr/kernel/sched.c");
        assert_eq!(consumers[0].size, 500);
        assert_eq!(consumers[1].path, "zephyr/kernel/thread.c");
        assert_eq!(consumers[1].size, 400);
        assert_eq!(consumers[2].path, "app/main.c");
        assert_eq!(consumers[2].size, 100);
    }

    #[test]
    fn test_flatten_tree_symbol_level() {
        let tree = sample_tree();
        let consumers = flatten_tree(&tree, "symbol");
        // Should get all 4 leaf symbols
        assert_eq!(consumers.len(), 4);
        // Sorted by size descending
        assert_eq!(consumers[0].size, 400); // k_thread_create
        assert_eq!(consumers[1].size, 300); // k_sched_unlock
        assert_eq!(consumers[2].size, 200); // k_sched_lock
        assert_eq!(consumers[3].size, 100); // main.c
    }

    #[test]
    fn test_flatten_tree_percentages() {
        let tree = sample_tree();
        let consumers = flatten_tree(&tree, "file");
        assert!((consumers[0].percent - 50.0).abs() < 0.01); // 500/1000
        assert!((consumers[1].percent - 40.0).abs() < 0.01); // 400/1000
        assert!((consumers[2].percent - 10.0).abs() < 0.01); // 100/1000
    }

    #[test]
    fn test_diff_trees() {
        let a = SizeReport {
            total_size: 1000,
            used_size: 1000,
            tree: SizeNode {
                name: "root".to_string(),
                size: 1000,
                children: vec![
                    SizeNode {
                        name: "file_a.c".to_string(),
                        size: 600,
                        children: vec![],
                    },
                    SizeNode {
                        name: "file_b.c".to_string(),
                        size: 400,
                        children: vec![],
                    },
                ],
            },
        };

        let b = SizeReport {
            total_size: 1200,
            used_size: 1200,
            tree: SizeNode {
                name: "root".to_string(),
                size: 1200,
                children: vec![
                    SizeNode {
                        name: "file_a.c".to_string(),
                        size: 800,
                        children: vec![],
                    },
                    SizeNode {
                        name: "file_b.c".to_string(),
                        size: 300,
                        children: vec![],
                    },
                    SizeNode {
                        name: "file_c.c".to_string(),
                        size: 100,
                        children: vec![],
                    },
                ],
            },
        };

        let (increases, decreases) = diff_trees(&a, &b, 10);

        // file_a grew by 200, file_c is new (+100)
        assert_eq!(increases.len(), 2);
        assert_eq!(increases[0].path, "file_a.c");
        assert_eq!(increases[0].delta, 200);
        assert_eq!(increases[1].path, "file_c.c");
        assert_eq!(increases[1].delta, 100);

        // file_b shrank by 100
        assert_eq!(decreases.len(), 1);
        assert_eq!(decreases[0].path, "file_b.c");
        assert_eq!(decreases[0].delta, -100);
    }

    #[test]
    fn test_diff_trees_limit() {
        let a = SizeReport {
            total_size: 100,
            used_size: 100,
            tree: SizeNode {
                name: "root".to_string(),
                size: 100,
                children: vec![
                    SizeNode { name: "a.c".to_string(), size: 50, children: vec![] },
                    SizeNode { name: "b.c".to_string(), size: 50, children: vec![] },
                ],
            },
        };

        let b = SizeReport {
            total_size: 200,
            used_size: 200,
            tree: SizeNode {
                name: "root".to_string(),
                size: 200,
                children: vec![
                    SizeNode { name: "a.c".to_string(), size: 100, children: vec![] },
                    SizeNode { name: "b.c".to_string(), size: 80, children: vec![] },
                    SizeNode { name: "c.c".to_string(), size: 20, children: vec![] },
                ],
            },
        };

        let (increases, _) = diff_trees(&a, &b, 1);
        assert_eq!(increases.len(), 1); // limited to 1
    }

    #[test]
    fn test_diff_trees_identical() {
        let report = SizeReport {
            total_size: 100,
            used_size: 100,
            tree: SizeNode {
                name: "root".to_string(),
                size: 100,
                children: vec![
                    SizeNode { name: "a.c".to_string(), size: 100, children: vec![] },
                ],
            },
        };

        let (increases, decreases) = diff_trees(&report, &report, 10);
        assert!(increases.is_empty());
        assert!(decreases.is_empty());
    }
}
