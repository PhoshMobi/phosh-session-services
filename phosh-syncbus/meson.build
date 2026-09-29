exe_target = custom_target(
  server_exe_name,
  build_by_default: true,
  build_always_stale: true,
  console: true,
  install: true,
  install_dir: libexec_dir,
  env: cargo_env,
  output: server_exe_name,
  command: [
    cargo,
    'build',
    cargo_args + ['-p', 'server'],
    '&&',
    'cp',
    meson.project_build_root() / 'target' / cargo_target / 'server',
    '@OUTPUT@',
  ],
)
