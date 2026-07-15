fn main() {
    let mut stderr = std::io::stderr();
    if std::env::var("MCPACE_KILL_PROCESS_TREE_ON_EXIT").as_deref() == Ok("1") {
        if let Err(error) = mcpace::enable_kill_on_exit_process_tree() {
            mcpace::write_startup_diagnostic(
                &mut stderr,
                &format!("failed to enable process-tree containment: {error}"),
            );
            std::process::exit(1);
        }
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut stdout = std::io::stdout();
    std::process::exit(mcpace::run(args, &mut stdout, &mut stderr));
}
