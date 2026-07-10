use std::fs;
use std::process::Command;

#[test]
fn release_binary_rejects_mock_auth_without_writing_a_pro_entitlement() {
    let entitlement_dir = std::env::temp_dir().join(format!(
        "audit-risk-mock-auth-release-{}",
        uuid::Uuid::new_v4()
    ));

    let output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args(["auth", "login"])
        .env("AUDIT_RISK_ENTITLEMENT_DIR", &entitlement_dir)
        .env("AUDIT_RISK_AUTH_BASE_URL", "mock://approved")
        .output()
        .expect("run audit-risk auth login");

    let entitlement_path = entitlement_dir.join("entitlement.json");
    let entitlement = fs::read_to_string(&entitlement_path).unwrap_or_default();
    let _ = fs::remove_dir_all(&entitlement_dir);

    assert!(
        !output.status.success(),
        "release binary must reject mock auth; stdout: {}; stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !entitlement.contains("\"plan\":\"pro_personal_monthly\""),
        "mock auth must never write a Pro entitlement",
    );
}
