use std::path::{Path, PathBuf};
use std::process::Command;

use hologram_engine::engine::{engine_analyze, engine_init};
use hologram_engine::routing::preflight::{load_baseline, run_full_check, save_baseline};

fn main() {
    let args = parse_args(std::env::args().skip(1).collect());
    let workspace = match args.workspace {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("usage: hologram-risk-check --workspace <path> [--pretty]");
            std::process::exit(2);
        }
    };

    match run_workspace_check(&workspace) {
        Ok(report) => {
            if args.pretty {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else {
                println!("{}", serde_json::to_string(&report).unwrap());
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

#[derive(Debug, Default)]
struct CliArgs {
    workspace: Option<String>,
    pretty: bool,
}

fn parse_args(args: Vec<String>) -> CliArgs {
    let mut output = CliArgs::default();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--workspace" => {
                if let Some(value) = args.get(index + 1) {
                    output.workspace = Some(value.clone());
                }
                index += 2;
            }
            "--pretty" => {
                output.pretty = true;
                index += 1;
            }
            _ => {
                index += 1;
            }
        }
    }

    output
}

fn run_workspace_check(workspace: &Path) -> Result<serde_json::Value, String> {
    if !workspace.exists() {
        return Err(format!("workspace does not exist: {}", workspace.display()));
    }

    let before = load_baseline(workspace);
    engine_init(workspace)?;
    let analysis = engine_analyze(workspace)?;
    let changed_files = git_changed_files(workspace);
    let result = run_full_check(
        &before,
        &analysis.graph,
        &changed_files,
        &workspace.to_string_lossy(),
    );
    save_baseline(workspace, &analysis.graph);

    Ok(serde_json::json!({
        "generated_at": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        "workspace_root": workspace.to_string_lossy(),
        "changed_files": changed_files,
        "analysis": {
            "node_count": analysis.node_count,
            "edge_count": analysis.edge_count,
            "community_count": analysis.community_count,
            "elapsed_secs": analysis.elapsed_secs,
        },
        "check": result,
    }))
}

fn git_changed_files(workspace: &Path) -> Vec<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace)
        .arg("status")
        .arg("--short")
        .output();

    let output = match output {
        Ok(value) if value.status.success() => value,
        _ => return Vec::new(),
    };

    parse_git_status_changed_files(&String::from_utf8_lossy(&output.stdout))
}

fn parse_git_status_changed_files(raw: &str) -> Vec<String> {
    let mut files = Vec::new();

    for line in raw.lines() {
        if line.len() < 4 {
            continue;
        }
        let path_part = line[3..].trim();
        if path_part.is_empty() {
            continue;
        }
        let normalized = if let Some((_, to)) = path_part.split_once("->") {
            to.trim()
        } else {
            path_part
        };
        if !normalized.is_empty() {
            files.push(normalized.replace('\\', "/"));
        }
    }

    files
}

#[cfg(test)]
mod tests {
    use super::{parse_args, parse_git_status_changed_files};

    #[test]
    fn parse_args_reads_workspace_and_pretty_flag() {
        let parsed = parse_args(vec![
            "--workspace".into(),
            "/tmp/repo".into(),
            "--pretty".into(),
        ]);

        assert_eq!(parsed.workspace.as_deref(), Some("/tmp/repo"));
        assert!(parsed.pretty);
    }

    #[test]
    fn parse_git_status_changed_files_handles_modified_untracked_and_rename_lines() {
        let files = parse_git_status_changed_files(
            " M src/auth.ts\n?? .hologram/delivery.json\nR  old/name.ts -> new/name.ts\n",
        );

        assert_eq!(files, vec![
            "src/auth.ts".to_string(),
            ".hologram/delivery.json".to_string(),
            "new/name.ts".to_string(),
        ]);
    }
}
