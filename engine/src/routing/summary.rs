use serde_json::{json, Value};

pub fn generate_summary(changed_files: &[String], violations: &[Value], coupling_l4: usize, cycle_count: usize) -> Value {
    let l5 = violations.iter().filter(|v| v["level"].as_u64() == Some(5)).count();
    let l4 = violations.iter().filter(|v| v["level"].as_u64() == Some(4)).count();
    let l3 = violations.iter().filter(|v| v["level"].as_u64() == Some(3)).count();
    let l2 = violations.iter().filter(|v| v["level"].as_u64() == Some(2)).count();
    let passed = violations.is_empty();
    let one_line = if passed {
        "No violations detected — all checks passed.".to_string()
    } else {
        let security = violations.iter().filter(|v| v["level"].as_u64() == Some(5)).count();
        let arch = violations.len() - security;
        match (security, arch) {
            (s, 0) => format!("发现 {} 条安全风险（密钥/注入/危险执行）", s),
            (0, a) => format!("发现 {} 条架构变更信号（配置/耦合）", a),
            (s, a) => format!("发现 {} 条安全风险 + {} 条架构变更信号", s, a),
        }
    };
    json!({
        "passed": passed, "one_line": one_line,
        "violations_by_level": { "5": l5, "4": l4, "3": l3, "2": l2 },
        "changed_files": changed_files, "total_violations": violations.len(),
        "coupling_l4": coupling_l4, "cycles": cycle_count
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_no_violations() {
        let r = generate_summary(&[], &[], 0, 0);
        assert!(r["passed"].as_bool().unwrap());
        assert_eq!(r["total_violations"], 0);
    }

    #[test]
    fn test_summary_with_violations() {
        let v = vec![
            json!({"level": 5, "desc": "migration changed"}),
            json!({"level": 4, "desc": "L4 coupling"}),
            json!({"level": 2, "desc": "cycle"}),
        ];
        let r = generate_summary(&["src/a.rs".into()], &v, 2, 1);
        assert!(!r["passed"].as_bool().unwrap());
        assert_eq!(r["total_violations"], 3);
        let by_level = &r["violations_by_level"];
        assert_eq!(by_level["5"], 1);
        assert_eq!(by_level["4"], 1);
        assert_eq!(by_level["3"], 0);
        assert_eq!(by_level["2"], 1);
    }

    #[test]
    fn test_summary_includes_coupling_and_cycles() {
        let r = generate_summary(&[], &[], 5, 3);
        assert_eq!(r["coupling_l4"], 5);
        assert_eq!(r["cycles"], 3);
    }
}
