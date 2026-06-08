// HoloGram Tauri Backend
// 桥接层：Agent (TypeScript) → Tauri commands → Python engine
// 不做分析逻辑，只做进程管理和文本转发

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;
use tauri::{Emitter, Manager};

// ═══════════════════════════════════════════════════════
// Python helpers
// ═══════════════════════════════════════════════════════

/// Find the Python executable with required dependencies.
fn python() -> String {
    let system_python = r"C:\Users\Administrator\AppData\Local\Python\pythoncore-3.14-64\python.exe";
    if std::path::Path::new(system_python).exists() {
        return system_python.to_string();
    }
    "python".to_string()
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or(PathBuf::from(".").as_path())
        .to_path_buf()
}

fn default_graph() -> String {
    project_root()
        .join("hologram_full.json")
        .to_string_lossy()
        .to_string()
}

/// Run a Python hologram CLI command and capture combined stdout+stderr.
fn run_hologram(args: &[&str]) -> Result<String, String> {
    let root = project_root();
    let output = Command::new(python())
        .current_dir(&root)
        .args(["-m", "src_python"])
        .args(args)
        .output()
        .map_err(|e| format!("Failed to spawn Python: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let mut result = String::new();
    if !stderr.is_empty() {
        result.push_str(&stderr);
        result.push('\n');
    }
    if !stdout.is_empty() {
        result.push_str(&stdout);
    }

    if !output.status.success() {
        return Err(if result.is_empty() {
            format!("Command failed with exit code {}", output.status)
        } else {
            result
        });
    }

    Ok(if result.is_empty() {
        "(no output)".into()
    } else {
        result
    })
}

/// Run inline Python code and return output.
fn run_python_code(code: &str) -> Result<String, String> {
    let root = project_root();
    let output = Command::new(python())
        .current_dir(&root)
        .args(["-c", code])
        .output()
        .map_err(|e| format!("Failed to spawn Python: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(format!("{}{}", stderr, stdout));
    }
    Ok(format!("{}{}", stdout, stderr))
}

// ═══════════════════════════════════════════════════════
// Watcher State
// ═══════════════════════════════════════════════════════

struct WatcherState {
    running: AtomicBool,
    project_path: Mutex<String>,
}

/// Collect mtimes of all Python/TypeScript/JS files under root.
fn collect_file_mtimes(root: &str) -> HashMap<String, u64> {
    let mut map = HashMap::new();
    let exts = [".py", ".pyi", ".ts", ".tsx", ".js", ".jsx", ".mjs"];
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() { continue; }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let ext_with_dot = format!(".{}", ext);
        if exts.contains(&ext_with_dot.as_str()) {
            if let Ok(meta) = path.metadata() {
                if let Ok(mtime) = meta.modified() {
                    if let Ok(dur) = mtime.duration_since(std::time::UNIX_EPOCH) {
                        map.insert(path.to_string_lossy().to_string(), dur.as_secs());
                    }
                }
            }
        }
    }
    map
}

/// Run incremental analysis for a project, return JSON.
fn run_incremental_analysis(project_path: &str) -> Option<String> {
    let root = project_root();
    let output = Command::new(python())
        .current_dir(&root)
        .args(["-m", "src_python", project_path, "--format", "json"])
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if !stdout.trim().is_empty() {
            return Some(stdout);
        }
    }
    None
}

// ═══════════════════════════════════════════════════════
// 13 Tauri commands — one per hologram tool
// ═══════════════════════════════════════════════════════

#[tauri::command]
async fn hologram_analyze(path: Option<String>) -> Result<String, String> {
    let target = path.unwrap_or_else(|| project_root().to_string_lossy().to_string());
    let graph_path = default_graph();
    run_hologram(&["analyze", &target, "-o", &graph_path])
}

#[tauri::command]
async fn hologram_neighbors(node_id: String, _depth: Option<i32>) -> Result<String, String> {
    let graph = default_graph();
    run_hologram(&["neighbors", &node_id, "-g", &graph])
}

#[tauri::command]
async fn hologram_impact(node_id: String, max_depth: Option<i32>) -> Result<String, String> {
    let graph = default_graph();
    let d = max_depth.unwrap_or(0);
    if d > 0 {
        run_hologram(&["impact", &node_id, "-d", &d.to_string(), "-g", &graph])
    } else {
        run_hologram(&["impact", &node_id, "-g", &graph])
    }
}

#[tauri::command]
async fn hologram_path(from: String, to: String) -> Result<String, String> {
    run_hologram(&["path", &from, &to, "-g", &default_graph()])
}

#[tauri::command]
async fn hologram_diff(before_path: String, after_path: Option<String>) -> Result<String, String> {
    let after = after_path.unwrap_or_else(default_graph);
    run_hologram(&["diff", &before_path, &after])
}

#[tauri::command]
async fn hologram_fragile(limit: Option<i32>) -> Result<String, String> {
    let l = limit.unwrap_or(10);
    run_hologram(&["fragile", "-l", &l.to_string(), "-g", &default_graph()])
}

#[tauri::command]
async fn hologram_cycle(mode: Option<String>) -> Result<String, String> {
    let m = mode.unwrap_or_else(|| "all".into());
    run_hologram(&["cycle", "-m", &m, "-g", &default_graph()])
}

#[tauri::command]
async fn hologram_coupling_report(module: String) -> Result<String, String> {
    run_hologram(&["coupling-report", &module, "-g", &default_graph()])
}

#[tauri::command]
async fn hologram_blindspots(threshold: Option<f64>) -> Result<String, String> {
    let t = threshold.unwrap_or(0.5);
    let root = project_root();
    let code = format!(
        r#"
import sys, json
sys.path.insert(0, r"{}")
from analysis.blindspots import find_blindspots
from core.graph import Graph
graph = Graph.from_json(r"{}")
results = find_blindspots(graph, min_confidence={})
print(json.dumps(results, indent=2, ensure_ascii=False))
"#,
        root.join("src_python").to_string_lossy(),
        default_graph(),
        t,
    );
    run_python_code(&code)
}

#[tauri::command]
async fn hologram_thread_conflicts(_severity: Option<String>) -> Result<String, String> {
    let root = project_root();
    let code = format!(
        r#"
import sys, json, os
sys.path.insert(0, r"{}")
from analysis.threading import thread_conflict_report
sources = {{}}
sp = r"{}"
for dirpath, _, filenames in os.walk(sp):
    for fn in filenames:
        if fn.endswith('.py'):
            fp = os.path.join(dirpath, fn)
            try:
                with open(fp, 'r', encoding='utf-8', errors='replace') as f:
                    sources[fp] = f.read()
            except: pass
result = thread_conflict_report(sources, language="python")
print(json.dumps(result, indent=2, ensure_ascii=False))
"#,
        root.join("src_python").to_string_lossy(),
        root.join("src_python").to_string_lossy(),
    );
    run_python_code(&code)
}

#[tauri::command]
async fn hologram_timeline(
    since: Option<String>,
    limit: Option<i32>,
    module: Option<String>,
) -> Result<String, String> {
    let root = project_root();
    let lim = limit.unwrap_or(50);
    let since_clause = since
        .map(|s| format!(" AND timestamp >= '{}'", s))
        .unwrap_or_default();
    let module_clause = module
        .map(|m| format!(" AND file LIKE '%{}%'", m))
        .unwrap_or_default();
    let code = format!(
        r#"
import sys, json
sys.path.insert(0, r"{}")
from timeline import TimelineStore
store = TimelineStore(r"{}")
rows = store.query(
    f"SELECT * FROM timeline WHERE 1=1 {{}} {{}} ORDER BY timestamp DESC LIMIT {{}}",
    '{}',
    '{}',
    {}
)
print(json.dumps(rows, indent=2, ensure_ascii=False, default=str))
store.close()
"#,
        root.join("src_python").to_string_lossy(),
        root.to_string_lossy(),
        since_clause,
        module_clause,
        lim,
    );
    run_python_code(&code)
}

#[tauri::command]
async fn hologram_community_report(
    resolution: Option<f64>,
    min_size: Option<i32>,
) -> Result<String, String> {
    let _ = resolution;
    let min = min_size.unwrap_or(3);
    let code = format!(
        r#"
import sys, json
sys.path.insert(0, r"{}")
from core.graph import Graph
from core.community import CommunityDetector
graph = Graph.from_json(r"{}")
detector = CommunityDetector()
communities = detector.detect(graph)
filtered = [c for c in communities if len(c.get('nodes', [])) >= {}]
print(json.dumps(filtered, indent=2, ensure_ascii=False))
"#,
        project_root().join("src_python").to_string_lossy(),
        default_graph(),
        min,
    );
    run_python_code(&code)
}

#[tauri::command]
async fn hologram_graph_summary() -> Result<String, String> {
    let code = format!(
        r#"
import sys, json
sys.path.insert(0, r"{}")
from core.graph import Graph
graph = Graph.from_json(r"{}")
nodes = list(graph.nodes.values())
edges = list(graph.edges.values())
node_types = {{}}
edge_types = {{}}
for n in nodes:
    nt = n.type.value if hasattr(n.type, 'value') else str(n.type)
    node_types[nt] = node_types.get(nt, 0) + 1
for e in edges:
    et = e.type.value if hasattr(e.type, 'value') else str(e.type)
    edge_types[et] = edge_types.get(et, 0) + 1
n = len(nodes)
density = round((2 * len(edges)) / (n * (n - 1)), 6) if n > 1 else 0
summary = {{
    "total_nodes": n,
    "total_edges": len(edges),
    "node_types": node_types,
    "edge_types": edge_types,
    "density": density,
    "communities": getattr(graph, 'community_count', 0),
    "top_node_kinds": sorted(node_types.items(), key=lambda x: x[1], reverse=True)[:10],
}}
print(json.dumps(summary, indent=2, ensure_ascii=False))
"#,
        project_root().join("src_python").to_string_lossy(),
        default_graph(),
    );
    run_python_code(&code)
}

// ═══════════════════════════════════════════════════════
// Graph loading — for star graph rendering
// ═══════════════════════════════════════════════════════

/// Load the graph JSON file and return it as a string.
#[tauri::command]
async fn load_graph_json(path: Option<String>) -> Result<String, String> {
    let p = path.unwrap_or_else(default_graph);
    std::fs::read_to_string(&p).map_err(|e| format!("Cannot read graph at {}: {e}", p))
}

/// Analyze a folder and return the graph JSON. Uses incremental cache.
#[tauri::command]
async fn analyze_and_load(path: String, app: tauri::AppHandle) -> Result<String, String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_title("全息观测站 — 分析中...");
    }

    let root = project_root();
    let python = python();

    let output = Command::new(&python)
        .current_dir(&root)
        .args(["-m", "src_python", &path, "--format", "json"])
        .output()
        .map_err(|e| format!("无法启动 Python:\n  Python: {python}\n  错误: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(format!(
            "分析失败 (exit code {}):\n--- stderr ---\n{}\n--- stdout ---\n{}",
            output.status,
            stderr,
            if stdout.len() > 500 { format!("{}...", &stdout[..500]) } else { stdout }
        ));
    }

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_title("全息观测站");
    }

    if stdout.trim().is_empty() {
        return Err(format!("分析完成但无输出。stderr:\n{}", stderr));
    }

    Ok(stdout)
}

// ═══════════════════════════════════════════════════════
// File Watcher — live incremental updates
// ═══════════════════════════════════════════════════════

#[tauri::command]
async fn start_watching(
    path: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<WatcherState>>,
) -> Result<(), String> {
    // Stop any existing watcher first
    state.running.store(false, Ordering::SeqCst);
    thread::sleep(Duration::from_millis(200));

    state.running.store(true, Ordering::SeqCst);
    *state.project_path.lock().unwrap() = path.clone();

    let watcher = state.inner().clone(); // Arc<WatcherState>
    let app_handle = app.clone();

    thread::spawn(move || {
        let mut last_mtimes = collect_file_mtimes(&path);
        let debounce = Duration::from_secs(3);

        while watcher.running.load(Ordering::SeqCst) {
            thread::sleep(debounce);

            if !watcher.running.load(Ordering::SeqCst) { break; }

            let current_mtimes = collect_file_mtimes(&path);

            // Check for any mtime changes or new/deleted files
            let mut changed = false;
            for (fp, mt) in &current_mtimes {
                match last_mtimes.get(fp) {
                    Some(old) if old != mt => { changed = true; break; }
                    None => { changed = true; break; } // new file
                    _ => {}
                }
            }
            if !changed {
                for fp in last_mtimes.keys() {
                    if !current_mtimes.contains_key(fp) {
                        changed = true; // deleted file
                        break;
                    }
                }
            }

            if changed {
                last_mtimes = current_mtimes;
                if let Some(json) = run_incremental_analysis(&path) {
                    let _ = app_handle.emit("graph-updated", json);
                }
            }
        }
    });

    Ok(())
}

#[tauri::command]
async fn stop_watching(
    state: tauri::State<'_, Arc<WatcherState>>,
) -> Result<(), String> {
    state.running.store(false, Ordering::SeqCst);
    Ok(())
}

// ═══════════════════════════════════════════════════════
// Main
// ═══════════════════════════════════════════════════════

fn main() {
    let watcher_state = Arc::new(WatcherState {
        running: AtomicBool::new(false),
        project_path: Mutex::new(String::new()),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(watcher_state)
        .invoke_handler(tauri::generate_handler![
            hologram_analyze,
            hologram_neighbors,
            hologram_impact,
            hologram_path,
            hologram_diff,
            hologram_fragile,
            hologram_cycle,
            hologram_coupling_report,
            hologram_blindspots,
            hologram_thread_conflicts,
            hologram_timeline,
            hologram_community_report,
            hologram_graph_summary,
            load_graph_json,
            analyze_and_load,
            start_watching,
            stop_watching,
        ])
        .run(tauri::generate_context!())
        .expect("error running hologram");
}
