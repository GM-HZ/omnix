use codex_arg0::Arg0DispatchPaths;

#[test]
fn default_dispatch_paths_do_not_assume_current_executable() {
    assert_eq!(Arg0DispatchPaths::default().codex_self_exe, None);
}
