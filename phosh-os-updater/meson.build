subdir('src')

service = 'phosh-os-updater'
custom_target(
  service,
  build_by_default: true,
  build_always_stale: true,
  console: true,
  install: true,
  install_dir: libexec_dir,
  env: cargo_env,
  output: service,
  command: [
    cargo,
    'build',
    cargo_args + ['-p', service],
    '&&',
    'cp',
    meson.project_build_root() / 'target' / cargo_target / service,
    '@OUTPUT@',
  ],
)
