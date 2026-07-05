fn main() {
    let exit_code = hologram_engine::cli::run_audit_risk_cli(
        std::env::args().skip(1).collect(),
    );
    std::process::exit(exit_code);
}
