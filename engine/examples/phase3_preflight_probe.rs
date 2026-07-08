use hologram_engine::graph::{Edge, EdgeKind, Graph, Node, NodeKind};
use hologram_engine::routing::preflight::run_full_check;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let scenario = args.first().map(String::as_str).unwrap_or("quiet");
    let changed_files: Vec<String> = if args.len() > 1 {
        args[1..].to_vec()
    } else {
        Vec::new()
    };

    let (before, after) = match scenario {
        "quiet" | "l5_config" | "l5_migration" => (Graph::new(), Graph::new()),
        "l4_coupling" => build_l4_coupling_graph(),
        other => {
            eprintln!("unknown phase3 probe scenario: {other}");
            std::process::exit(2);
        }
    };

    let result = run_full_check(&before, &after, &changed_files, ".");
    println!("{}", serde_json::to_string_pretty(&result).unwrap());
}

fn build_l4_coupling_graph() -> (Graph, Graph) {
    let before = Graph::new();
    let mut after = Graph::new();

    let mut source = Node::new("phase3:a", "phase3_source", NodeKind::Symbol);
    source.location = Some("src-ui/src/risk/phase3-evidence.ts".into());
    after.add_node(source);

    let mut target = Node::new("phase3:b", "phase3_target", NodeKind::Symbol);
    target.location = Some("src-ui/src/risk/phase3-evidence.ts".into());
    after.add_node(target);

    let mut edge = Edge::new("phase3:e1", "phase3:a", "phase3:b", EdgeKind::Calls);
    edge.coupling_depth = 4;
    after.add_edge(edge);

    (before, after)
}
