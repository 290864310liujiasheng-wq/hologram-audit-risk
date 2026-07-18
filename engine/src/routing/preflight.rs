use crate::analysis::{coupling_report, detect_cycles, thread_conflict_report};
use crate::community::louvain::detect_communities;
use crate::graph::{Graph, NodeKind};
use crate::routing::{
    constraints::{check_constraints, ConstraintConfig},
    signals::SignalGenerator,
    summary::generate_summary,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

/// Path to the per-project graph snapshot used as the briefing baseline.
pub fn baseline_path(project_root: &Path) -> PathBuf {
    project_root.join(".hologram").join("baseline.json")
}

pub fn load_baseline(project_root: &Path) -> Graph {
    let path = baseline_path(project_root);
    match std::fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str::<Graph>(&raw) {
            Ok(baseline) => baseline,
            Err(error) => {
                eprintln!(
                    "警告：.hologram/baseline.json 损坏或格式不兼容（{error}），本次以无基线模式运行。如需重置，删除该文件后重新运行 check。"
                );
                Graph::default()
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Graph::default(),
        Err(error) => {
            eprintln!("警告：无法读取 .hologram/baseline.json（{error}），本次以无基线模式运行。");
            Graph::default()
        }
    }
}

pub fn save_baseline(project_root: &Path, graph: &Graph) {
    let dir = project_root.join(".hologram");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(json) = serde_json::to_string_pretty(graph) {
        let _ = std::fs::write(baseline_path(project_root), json);
    }
}

/// Full CheckResult properties for timeline round-trip (historical briefing click).
pub fn check_timeline_props(result: &Value) -> Value {
    json!({
        "passed": result["passed"],
        "timestamp": result["timestamp"],
        "changed_files": result["changed_files"],
        "total_changed_files": result["total_changed_files"],
        "l5_violations": result["l5_violations"],
        "l4_violations": result["l4_violations"],
        "l3_violations": result["l3_violations"],
        "l2_violations": result["l2_violations"],
        "passed_checks": result["passed_checks"],
        "blast_radius": result["blast_radius"],
        "cross_community_edges": result["cross_community_edges"],
        "new_cycles": result["new_cycles"],
        "new_thread_conflicts": result["new_thread_conflicts"],
        "api_signature_changes": result["api_signature_changes"],
        "scanned_file_count": result["scanned_file_count"],
        "violation_count": result["violation_count"],
    })
}

const CHECKED_RISK_CATEGORIES: &str =
    "硬编码密钥、SQL注入、危险动态执行、IAM通配符等风险";

fn no_findings_summary(scanned_file_count: usize) -> String {
    format!(
        "已扫描 {scanned_file_count} 个文件，检查{CHECKED_RISK_CATEGORIES}，未发现问题"
    )
}

fn quiet_check_result(
    changed_files: &[String],
    one_line: &str,
    baseline_seed: bool,
    scanned_file_count: usize,
) -> Value {
    json!({
        "passed": true,
        "one_line": one_line,
        "timestamp": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        "changed_files": changed_files,
        "total_changed_files": changed_files.len(),
        "l5_violations": [],
        "l4_violations": [],
        "l3_violations": [],
        "l2_violations": [],
        "passed_checks": Vec::<String>::new(),
        "blast_radius": 0u32,
        "cross_community_edges": 0u32,
        "new_cycles": 0u32,
        "new_thread_conflicts": 0u32,
        "api_signature_changes": 0u32,
        "coupling_l4": 0u32,
        "cycles_detected": 0u32,
        "signals_count": 0u32,
        "scanned_file_count": scanned_file_count,
        "violation_count": 0u32,
        "quiet": !baseline_seed,
        "baseline_seed": baseline_seed,
    })
}

/// Build a check result from a full-workspace per-file scan (no diff context).
/// Only per-file findings (leaked keys / injection / dangerous exec / IAM) are
/// reported; architectural metrics are zeroed because there is no baseline to
/// diff against. Used on the very first `check` so an existing codebase's risks
/// are surfaced instead of a false "0 findings".
fn full_scan_result(secret_signals: &[Value], scanned_file_count: usize) -> Value {
    let config = ConstraintConfig::defaults();
    let constraint_result = check_constraints(secret_signals, &config);
    let violations: Vec<Value> = constraint_result["violations"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let no_files: Vec<String> = Vec::new();
    let summary = generate_summary(&no_files, &violations, 0, 0);
    json!({
        "passed": summary["passed"],
        "one_line": summary["one_line"],
        "timestamp": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        "changed_files": no_files,
        "total_changed_files": 0,
        "l5_violations": violations.iter().filter(|v| v["level"]==5).collect::<Vec<_>>(),
        "l4_violations": violations.iter().filter(|v| v["level"]==4).collect::<Vec<_>>(),
        "l3_violations": violations.iter().filter(|v| v["level"]==3).collect::<Vec<_>>(),
        "l2_violations": violations.iter().filter(|v| v["level"]==2).collect::<Vec<_>>(),
        "passed_checks": Vec::<String>::new(),
        "blast_radius": 0u32,
        "cross_community_edges": 0u32,
        "new_cycles": 0u32,
        "new_thread_conflicts": 0u32,
        "api_signature_changes": 0u32,
        "coupling_l4": 0u32,
        "cycles_detected": 0u32,
        "signals_count": secret_signals.len() as u32,
        "scanned_file_count": scanned_file_count,
        "violation_count": violations.len() as u32,
        "full_scan": true,
    })
}

/// True for files audit-risk manages itself (its own state dir and generated
/// automation). Changes to these are the tool bookkeeping its own scaffolding,
/// not user-facing risk — flagging `.hologram/delivery.json` or the generated
/// CI workflow as a "critical config change" is just noise that buries the
/// real findings (leaked keys, injection) further down the list.
fn is_tool_artifact(path: &str) -> bool {
    let p = path.trim_start_matches("./");
    p == ".hologram"
        || p.starts_with(".hologram/")
        || p == ".githooks/pre-commit"
        || p == ".github/workflows/hologram-risk.yml"
}

/// run_full_check — equivalent of Python preflight.py run_full_check()
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FullCheckOptions {
    pub initial_full_scan: bool,
    pub include_global_graph_deltas: bool,
}

pub fn run_full_check(
    before: &Graph,
    after: &Graph,
    changed_files: &[String],
    project_root: &str,
) -> Value {
    run_full_check_with_options(
        before,
        after,
        changed_files,
        project_root,
        FullCheckOptions {
            initial_full_scan: true,
            include_global_graph_deltas: true,
        },
    )
}

pub fn run_full_check_with_options(
    before: &Graph,
    after: &Graph,
    changed_files: &[String],
    project_root: &str,
    options: FullCheckOptions,
) -> Value {
    // Drop audit-risk's own managed files so its scaffolding never shows up as
    // user risk. Everything below sees only genuine workspace changes.
    let filtered_changed: Vec<String> = changed_files
        .iter()
        .filter(|f| !is_tool_artifact(f))
        .cloned()
        .collect();
    let changed_files: &[String] = &filtered_changed;

    // First open (no baseline yet) with nothing in the diff: run a FULL workspace
    // scan for per-file risks (leaked keys, injection, dangerous exec, IAM) so an
    // existing codebase's risks surface on the very first `check`, instead of a
    // dangerous "0 findings" just because nothing changed since baseline. The
    // architectural diff checks genuinely need a baseline, so they stay diff-based.
    if before.nodes.is_empty() && changed_files.is_empty() {
        if !options.initial_full_scan {
            return quiet_check_result(
                changed_files,
                "本次选择了 0 个文件，未执行风险扫描，基线已建立",
                true,
                0,
            );
        }
        let workspace_scan = crate::routing::secrets::scan_workspace(project_root);
        if workspace_scan.signals.is_empty() {
            return quiet_check_result(
                changed_files,
                &no_findings_summary(workspace_scan.scanned_file_count),
                true,
                workspace_scan.scanned_file_count,
            );
        }
        return full_scan_result(
            &workspace_scan.signals,
            workspace_scan.scanned_file_count,
        );
    }

    // No file changes and graph size unchanged → nothing to report.
    if changed_files.is_empty()
        && before.node_count() == after.node_count()
        && before.edge_count() == after.edge_count()
    {
        return quiet_check_result(changed_files, "无新变更", false, 0);
    }

    let coupling = coupling_report(after, ""); // full graph
    let l4_count = coupling["L4"].as_u64().unwrap_or(0) as usize;
    let cycles = detect_cycles(after);
    let cycle_count = cycles.len();
    let cycles_before = detect_cycles(before).len();
    let mut signals = SignalGenerator::new().generate(
        before,
        after,
        changed_files,
        project_root,
        l4_count,
        cycle_count,
    );
    if !options.include_global_graph_deltas {
        signals.retain(|signal| {
            signal["signal"]["file_path"]
                .as_str()
                .is_some_and(|file_path| !file_path.is_empty())
        });
    }

    // Secret scanning: scan the content of every changed file.
    // `changed_files` are relative to `project_root` (from `git status --short`
    // inside the workspace dir) — they must be resolved against project_root,
    // not the current process cwd, or every read silently misses.
    let project_root_path = Path::new(project_root);
    let absolute_changed_files: Vec<String> = if project_root.is_empty() {
        changed_files.to_vec()
    } else {
        changed_files
            .iter()
            .map(|f| project_root_path.join(f).to_string_lossy().into_owned())
            .collect()
    };
    let changed_scan =
        crate::routing::secrets::scan_changed_files(&absolute_changed_files, changed_files);
    let scanned_file_count = changed_scan.scanned_file_count;
    signals.extend(changed_scan.signals);
    let config = ConstraintConfig::defaults();
    let constraint_result = check_constraints(&signals, &config);
    let violations: Vec<Value> = constraint_result["violations"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let summary = generate_summary(changed_files, &violations, l4_count, cycle_count);

    // ── blast_radius: BFS from all nodes whose file is in changed_files ──
    let blast_radius = if changed_files.is_empty() {
        0usize
    } else {
        let mut seed_nodes: HashSet<&str> = HashSet::new();
        for node in after.nodes.values() {
            if let Some(ref loc) = node.location {
                if changed_files
                    .iter()
                    .any(|f| loc.starts_with(f.as_str()) || loc.contains(f.as_str()))
                {
                    seed_nodes.insert(node.id.as_str());
                }
            }
        }
        // BFS up to depth 3 from seed nodes
        let mut visited: HashSet<&str> = HashSet::new();
        let mut queue = VecDeque::new();
        for &sid in &seed_nodes {
            visited.insert(sid);
            queue.push_back((sid, 0usize));
        }
        // Build adjacency
        let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
        for edge in after.edges.values() {
            adj.entry(&edge.source).or_default().push(&edge.target);
            adj.entry(&edge.target).or_default().push(&edge.source);
        }
        while let Some((cur, depth)) = queue.pop_front() {
            if depth >= 3 {
                continue;
            }
            if let Some(nbs) = adj.get(cur) {
                for &nb in nbs {
                    if visited.insert(nb) {
                        queue.push_back((nb, depth + 1));
                    }
                }
            }
        }
        visited.len().saturating_sub(seed_nodes.len()) // exclude seeds themselves
    };

    // ── cross_community_edges: communities on after graph ──
    let communities = detect_communities(after, 42);
    let mut node_to_comm: HashMap<&str, usize> = HashMap::new();
    for (ci, comm) in communities.iter().enumerate() {
        for nid in comm {
            node_to_comm.insert(nid.as_str(), ci);
        }
    }
    let cross_community_edges = after
        .edges
        .values()
        .filter(|e| {
            let sc = node_to_comm.get(e.source.as_str());
            let tc = node_to_comm.get(e.target.as_str());
            sc != tc || sc.is_none()
        })
        .count();

    // ── thread_conflicts ──
    let thread_report = thread_conflict_report(after, changed_files);
    let new_thread_conflicts = thread_report["conflict_count"].as_u64().unwrap_or(0) as u32;

    // ── api_signature_changes: count function/method nodes changed ──
    let api_signature_changes = if before.nodes.is_empty() {
        0u32
    } else {
        let mut changed = 0u32;
        for (nid, after_node) in after.nodes.iter() {
            if !matches!(after_node.kind, NodeKind::Symbol) {
                continue;
            }
            if let Some(before_node) = before.nodes.get(nid) {
                // Count as changed if in/out degree differs
                if before_node.out_degree != after_node.out_degree
                    || before_node.in_degree != after_node.in_degree
                {
                    changed += 1;
                }
            } else {
                // New symbol node
                changed += 1;
            }
        }
        changed
    };

    json!({
        "passed": summary["passed"],
        "one_line": summary["one_line"],
        "timestamp": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        "changed_files": changed_files,
        "total_changed_files": changed_files.len(),
        "l5_violations": violations.iter().filter(|v| v["level"]==5).collect::<Vec<_>>(),
        "l4_violations": violations.iter().filter(|v| v["level"]==4).collect::<Vec<_>>(),
        "l3_violations": violations.iter().filter(|v| v["level"]==3).collect::<Vec<_>>(),
        "l2_violations": violations.iter().filter(|v| v["level"]==2).collect::<Vec<_>>(),
        "passed_checks": Vec::<String>::new(),
        "blast_radius": blast_radius as u32,
        "cross_community_edges": cross_community_edges as u32,
        "new_cycles": cycle_count.saturating_sub(cycles_before) as u32,
        "new_thread_conflicts": new_thread_conflicts,
        "api_signature_changes": api_signature_changes,
        "coupling_l4": l4_count as u32,
        "cycles_detected": cycle_count as u32,
        "signals_count": signals.len() as u32,
        "scanned_file_count": scanned_file_count,
        "violation_count": violations.len() as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, EdgeKind, Node, NodeKind};

    #[test]
    fn test_preflight_empty_graphs() {
        // Empty workspace (no risks) → quiet pass. Use a fresh temp dir as the
        // project root so the first-open full scan finds nothing (passing "."
        // would scan the engine's own corpus of intentional secrets).
        let dir = std::env::temp_dir().join(format!(
            "audit_preflight_empty-{}",
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let g = Graph::new();
        let r = run_full_check(&g, &g, &[], &dir.to_string_lossy());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(r["passed"].as_bool().unwrap());
        assert_eq!(r["blast_radius"], 0);
        assert_eq!(r["violation_count"], 0);
    }

    #[test]
    fn test_preflight_no_changes() {
        let mut g = Graph::new();
        g.add_node(Node::new("a", "fn_a", NodeKind::Symbol));
        g.add_node(Node::new("b", "fn_b", NodeKind::Symbol));
        g.add_edge(Edge::new("e1", "a", "b", EdgeKind::Calls));

        let r = run_full_check(&g, &g, &[], ".");
        assert!(r["passed"].as_bool().unwrap());
        assert_eq!(r["blast_radius"], 0);
    }

    #[test]
    fn test_preflight_changed_files_reports_actual_scanned_count() {
        let dir = std::env::temp_dir().join(format!(
            "audit_preflight_changed_count-{}",
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("new.rs"), "pub fn safe() {}\n").unwrap();
        let graph = Graph::new();
        let changed_files = vec!["new.rs".to_string(), "missing.rs".to_string()];

        let result = run_full_check(&graph, &graph, &changed_files, &dir.to_string_lossy());
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(result["scanned_file_count"], 1);
    }

    #[test]
    fn test_preflight_detects_l5_on_migration() {
        let g = Graph::new();
        let r = run_full_check(&g, &g, &["migrations/0001_init.py".into()], ".");
        assert!(!r["passed"].as_bool().unwrap());
        assert!(r["violation_count"].as_u64().unwrap() > 0);
    }

    #[test]
    fn test_preflight_blast_radius_with_changes() {
        let mut g = Graph::new();
        let mut a = Node::new("a", "mod_a", NodeKind::Symbol);
        a.location = Some("src/handler.rs".into());
        g.add_node(a);
        let mut b = Node::new("b", "mod_b", NodeKind::Symbol);
        b.location = Some("src/handler.rs".into());
        g.add_node(b);
        g.add_node(Node::new("c", "mod_c", NodeKind::Symbol));
        g.add_edge(Edge::new("e1", "a", "c", EdgeKind::Calls));
        g.add_edge(Edge::new("e2", "c", "b", EdgeKind::Calls));

        let r = run_full_check(&g, &g, &["src/handler.rs".into()], ".");
        // BFS from a,b should include c within depth 3
        assert!(r["blast_radius"].as_u64().unwrap() > 0);
    }

    #[test]
    fn test_preflight_api_signature_changes() {
        let mut before = Graph::new();
        let mut a = Node::new("a", "fn_a", NodeKind::Symbol);
        a.out_degree = 1;
        before.add_node(a);

        let mut after = Graph::new();
        let mut a2 = Node::new("a", "fn_a", NodeKind::Symbol);
        a2.out_degree = 3; // changed
        after.add_node(a2);
        let mut b = Node::new("b", "fn_b", NodeKind::Symbol);
        b.out_degree = 1;
        after.add_node(b);

        let r = run_full_check(&before, &after, &["src/a.rs".into()], ".");
        assert_eq!(r["api_signature_changes"], 2, "a changed + b new = 2");
    }

    #[test]
    fn test_preflight_stable_cycles_no_false_alarm() {
        let mut g = Graph::new();
        g.add_node(Node::new("a", "a", NodeKind::Symbol));
        g.add_node(Node::new("b", "b", NodeKind::Symbol));
        g.add_node(Node::new("c", "c", NodeKind::Symbol));
        g.add_edge(Edge::new("e1", "a", "b", EdgeKind::Calls));
        g.add_edge(Edge::new("e2", "b", "c", EdgeKind::Calls));
        g.add_edge(Edge::new("e3", "c", "a", EdgeKind::Calls));
        let r = run_full_check(&g, &g, &[], ".");
        assert!(r["passed"].as_bool().unwrap());
        assert_eq!(r["violation_count"], 0);
    }

    #[test]
    fn test_preflight_baseline_seed() {
        // First open on a clean workspace → seed baseline quietly. Fresh temp
        // dir as project root so the full scan finds no risks.
        let dir = std::env::temp_dir().join(format!(
            "audit_preflight_seed-{}",
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("README.md"), "# Safe workspace\n").unwrap();
        std::fs::write(dir.join("src/lib.rs"), "pub fn safe() {}\n").unwrap();
        let mut after = Graph::new();
        after.add_node(Node::new("a", "fn", NodeKind::Symbol));
        let before = Graph::new();
        let r = run_full_check(&before, &after, &[], &dir.to_string_lossy());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(r["passed"].as_bool().unwrap());
        assert_eq!(r["baseline_seed"], true);
        assert_eq!(r["violation_count"], 0);
        assert_eq!(r["scanned_file_count"], 2);
        assert_eq!(
            r["one_line"],
            "已扫描 2 个文件，检查硬编码密钥、SQL注入、危险动态执行、IAM通配符等风险，未发现问题"
        );
    }

    #[test]
    fn test_preflight_explicit_empty_selection_skips_initial_full_scan() {
        let dir = std::env::temp_dir().join(format!(
            "audit_preflight_selected_empty-{}",
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("leak.py"),
            "OPENAI_API_KEY = \"sk-proj-AbCdEf0123456789GhIjKlMnOpQrStUvWxYz012345\"\n",
        )
        .unwrap();
        let before = Graph::new();
        let after = Graph::new();

        let r = run_full_check_with_options(
            &before,
            &after,
            &[],
            &dir.to_string_lossy(),
            FullCheckOptions {
                initial_full_scan: false,
                include_global_graph_deltas: false,
            },
        );
        let _ = std::fs::remove_dir_all(&dir);

        assert!(r["passed"].as_bool().unwrap());
        assert_eq!(r["violation_count"], 0);
        assert_eq!(r["baseline_seed"], true);
        assert_eq!(r["scanned_file_count"], 0);
        assert_eq!(
            r["one_line"],
            "本次选择了 0 个文件，未执行风险扫描，基线已建立"
        );
        assert!(r.get("full_scan").is_none());
    }

    fn graph_with_new_global_l4_and_cycle() -> (Graph, Graph) {
        let mut before = Graph::new();
        for id in ["a", "b", "c"] {
            before.add_node(Node::new(id, id, NodeKind::Symbol));
        }
        before.add_edge(Edge::new("e1", "a", "b", EdgeKind::Calls));
        before.add_edge(Edge::new("e2", "b", "c", EdgeKind::Calls));

        let mut after = before.clone();
        let mut global_l4_cycle_edge = Edge::new("e3", "c", "a", EdgeKind::Calls);
        global_l4_cycle_edge.coupling_depth = 4;
        after.add_edge(global_l4_cycle_edge);
        (before, after)
    }

    #[test]
    fn test_preflight_explicit_selection_excludes_global_graph_deltas() {
        let (before, after) = graph_with_new_global_l4_and_cycle();
        let selected_files = vec!["src/safe.rs".to_string()];

        let result = run_full_check_with_options(
            &before,
            &after,
            &selected_files,
            ".",
            FullCheckOptions {
                initial_full_scan: false,
                include_global_graph_deltas: false,
            },
        );

        assert_eq!(result["l4_violations"], json!([]));
        assert_eq!(result["l2_violations"], json!([]));
    }

    #[test]
    fn test_preflight_default_check_includes_global_graph_deltas() {
        let (before, after) = graph_with_new_global_l4_and_cycle();

        let result = run_full_check(
            &before,
            &after,
            &["src/safe.rs".to_string()],
            ".",
        );

        assert_eq!(result["l4_violations"].as_array().unwrap().len(), 1);
        assert_eq!(result["l2_violations"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_preflight_first_open_scans_existing_risks() {
        // Regression guard: the first `check` on an existing codebase that
        // already contains a leaked key must NOT report a false "all clear" just
        // because nothing changed since baseline — it must full-scan and flag it.
        let dir = std::env::temp_dir().join(format!(
            "audit_preflight_fullscan-{}",
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("leak.py"),
            "OPENAI_API_KEY = \"sk-proj-AbCdEf0123456789GhIjKlMnOpQrStUvWxYz012345\"\n",
        )
        .unwrap();
        let before = Graph::new();
        let mut after = Graph::new();
        after.add_node(Node::new("a", "fn", NodeKind::Symbol));
        let r = run_full_check(&before, &after, &[], &dir.to_string_lossy());
        assert!(!r["passed"].as_bool().unwrap(), "leaked key must not pass");
        assert!(r["violation_count"].as_u64().unwrap() >= 1);
        assert_eq!(r["full_scan"], true);
    }
}
