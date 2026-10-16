exe_target = custom_target(
  push_distributor_exe_name,
  build_by_default: true,
  build_always_stale: true,
  console: true,
  install: true,
  install_dir: libexec_dir,
  env: cargo_env,
  output: push_distributor_exe_name,
  command: [
    cargo,
    'build',
    cargo_args + ['-p', 'phosh-push-distributor'],
    '&&',
    'cp',
    meson.project_build_root() / 'target' / cargo_target / push_distributor_exe_name,
    '@OUTPUT@',
  ],
)
