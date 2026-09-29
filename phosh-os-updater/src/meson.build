conf = configuration_data()
conf.set_quoted('GETTEXT_PACKAGE', meson.project_name())
conf.set_quoted('LOCALEDIR', locale_dir)

config_rs = 'config.rs'
configure_file(input: f'@config_rs@.in', output: config_rs, configuration: conf)

# Copy the config.rs output to the source directory.
run_command(
  'cp',
  meson.current_build_dir() / config_rs,
  meson.current_source_dir() / config_rs,
  check: true,
)
