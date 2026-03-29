use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::error::{MhostError, Result};

// ---------------------------------------------------------------------------
// GroupConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupConfig {
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub processes: Vec<String>,
}

// ---------------------------------------------------------------------------
// topological_sort
// ---------------------------------------------------------------------------

/// Returns groups in dependency order (dependencies first) using Kahn's
/// algorithm.  The output is deterministic because the ready-queue is always
/// sorted before each step.
///
/// # Errors
/// - Returns an error if a dependency names a group that does not exist.
/// - Returns an error if a cycle is detected.
pub fn topological_sort(groups: &HashMap<String, GroupConfig>) -> Result<Vec<String>> {
    // Validate all dependency references first.
    for (name, cfg) in groups {
        for dep in &cfg.depends_on {
            if !groups.contains_key(dep) {
                return Err(MhostError::Config(format!(
                    "Group '{}' depends on unknown group '{}'",
                    name, dep
                )));
            }
        }
    }

    // Build in-degree map and adjacency list (dep → dependents).
    let mut in_degree: HashMap<&str, usize> = groups.keys().map(|k| (k.as_str(), 0)).collect();
    let mut dependents: HashMap<&str, Vec<&str>> = groups.keys().map(|k| (k.as_str(), vec![])).collect();

    for (name, cfg) in groups {
        for dep in &cfg.depends_on {
            *in_degree.entry(name.as_str()).or_insert(0) += 1;
            dependents.entry(dep.as_str()).or_default().push(name.as_str());
        }
    }

    // Seed the queue with all nodes that have no dependencies.
    let mut queue: VecDeque<&str> = {
        let mut ready: Vec<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&name, _)| name)
            .collect();
        ready.sort_unstable();
        VecDeque::from(ready)
    };

    let mut order: Vec<String> = Vec::with_capacity(groups.len());

    while let Some(current) = queue.pop_front() {
        order.push(current.to_string());

        // Collect newly-ready dependents, sort for determinism, then enqueue.
        let mut newly_ready: Vec<&str> = Vec::new();
        if let Some(deps) = dependents.get(current) {
            for &dependent in deps {
                let deg = in_degree.entry(dependent).or_insert(0);
                *deg -= 1;
                if *deg == 0 {
                    newly_ready.push(dependent);
                }
            }
        }
        newly_ready.sort_unstable();
        for node in newly_ready {
            queue.push_back(node);
        }
    }

    if order.len() != groups.len() {
        return Err(MhostError::Config(
            "Cycle detected in group dependency graph".to_string(),
        ));
    }

    Ok(order)
}

// ---------------------------------------------------------------------------
// transitive_deps
// ---------------------------------------------------------------------------

/// Returns the set of all transitive dependency group names for `start` (not
/// including `start` itself) via depth-first search.
pub fn transitive_deps<'a>(
    start: &'a str,
    groups: &'a HashMap<String, GroupConfig>,
) -> HashSet<&'a str> {
    let mut visited: HashSet<&str> = HashSet::new();
    let mut stack: Vec<&str> = Vec::new();

    if let Some(cfg) = groups.get(start) {
        for dep in &cfg.depends_on {
            stack.push(dep.as_str());
        }
    }

    while let Some(current) = stack.pop() {
        if visited.insert(current) {
            if let Some(cfg) = groups.get(current) {
                for dep in &cfg.depends_on {
                    if !visited.contains(dep.as_str()) {
                        stack.push(dep.as_str());
                    }
                }
            }
        }
    }

    visited
}

// ---------------------------------------------------------------------------
// ordered_processes_for_group
// ---------------------------------------------------------------------------

/// Returns the processes that must run for `group_name`, in dependency order
/// (processes of dependencies first, then processes of `group_name` itself).
///
/// # Errors
/// - Propagates errors from `topological_sort`.
/// - Returns an error if `group_name` is not found.
pub fn ordered_processes_for_group(
    group_name: &str,
    groups: &HashMap<String, GroupConfig>,
) -> Result<Vec<String>> {
    if !groups.contains_key(group_name) {
        return Err(MhostError::Config(format!(
            "Group '{}' not found",
            group_name
        )));
    }

    let sorted_all = topological_sort(groups)?;

    // Collect all groups that are transitive deps of group_name (plus itself).
    let dep_set = transitive_deps(group_name, groups);

    let mut processes: Vec<String> = Vec::new();

    for g in &sorted_all {
        if g == group_name || dep_set.contains(g.as_str()) {
            if let Some(cfg) = groups.get(g) {
                processes.extend(cfg.processes.iter().cloned());
            }
        }
    }

    Ok(processes)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_groups() -> HashMap<String, GroupConfig> {
        let mut groups = HashMap::new();
        groups.insert(
            "database".to_string(),
            GroupConfig {
                depends_on: vec![],
                processes: vec!["postgres".to_string(), "redis".to_string()],
            },
        );
        groups.insert(
            "backend".to_string(),
            GroupConfig {
                depends_on: vec!["database".to_string()],
                processes: vec!["api-server".to_string()],
            },
        );
        groups.insert(
            "frontend".to_string(),
            GroupConfig {
                depends_on: vec!["backend".to_string()],
                processes: vec!["web-app".to_string()],
            },
        );
        groups
    }

    #[test]
    fn test_topological_order_database_before_backend_before_frontend() {
        let groups = make_groups();
        let order = topological_sort(&groups).expect("sort");

        let idx = |name: &str| order.iter().position(|g| g == name).unwrap();

        assert!(
            idx("database") < idx("backend"),
            "database must come before backend"
        );
        assert!(
            idx("backend") < idx("frontend"),
            "backend must come before frontend"
        );
    }

    #[test]
    fn test_cycle_detection() {
        let mut groups = HashMap::new();
        groups.insert(
            "a".to_string(),
            GroupConfig {
                depends_on: vec!["b".to_string()],
                processes: vec![],
            },
        );
        groups.insert(
            "b".to_string(),
            GroupConfig {
                depends_on: vec!["c".to_string()],
                processes: vec![],
            },
        );
        groups.insert(
            "c".to_string(),
            GroupConfig {
                depends_on: vec!["a".to_string()],
                processes: vec![],
            },
        );

        let result = topological_sort(&groups);
        assert!(result.is_err(), "expected cycle error");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("Cycle"),
            "error message should mention cycle, got: {}",
            msg
        );
    }

    #[test]
    fn test_unknown_dependency_error() {
        let mut groups = HashMap::new();
        groups.insert(
            "frontend".to_string(),
            GroupConfig {
                depends_on: vec!["nonexistent".to_string()],
                processes: vec!["web-app".to_string()],
            },
        );

        let result = topological_sort(&groups);
        assert!(result.is_err(), "expected unknown dep error");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("nonexistent"),
            "error message should mention missing dep, got: {}",
            msg
        );
    }

    #[test]
    fn test_ordered_processes_for_group() {
        let groups = make_groups();
        let procs =
            ordered_processes_for_group("frontend", &groups).expect("ordered_processes");

        // All processes from the full chain must appear.
        assert!(procs.contains(&"postgres".to_string()));
        assert!(procs.contains(&"redis".to_string()));
        assert!(procs.contains(&"api-server".to_string()));
        assert!(procs.contains(&"web-app".to_string()));

        // Dependencies must come before their dependents.
        let idx = |p: &str| procs.iter().position(|x| x == p).unwrap();
        assert!(idx("postgres") < idx("api-server"), "db procs before backend");
        assert!(idx("api-server") < idx("web-app"), "backend procs before frontend");
    }

    #[test]
    fn test_group_config_serialization_roundtrip() {
        let original = GroupConfig {
            depends_on: vec!["database".to_string()],
            processes: vec!["api-server".to_string(), "worker".to_string()],
        };

        let json = serde_json::to_string(&original).expect("serialize");
        let decoded: GroupConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, decoded);
    }
}
