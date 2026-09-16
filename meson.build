project(
  'phosh-session-services',
  'rust',
  version: '0.2.0',
  meson_version: '>= 1.1',
)

gnome = import('gnome')
i18n = import('i18n')

services = [
  # service name / app-id / descriptive name / service type
  [
    'phosh-os-updater',
    'mobi.phosh.OsUpdater',
    'Phosh OS Update Notifier',
    'simple',
  ],
  [
    'phosh-syncbus',
    'mobi.phosh.syncbus',
    'A D-Bus server for Syncthing.',
    'dbus',
  ],
]

server_exe_name = 'phosh-syncbus'
demo_exe_name = 'phosh-syncbus-demo'

conf_data = configuration_data()

prefix = get_option('prefix')
data_dir = prefix / get_option('datadir')
lib_dir = get_option('libdir')
libexec_dir = prefix / get_option('libexecdir')
locale_dir = prefix / get_option('localedir')

dbus_dir = data_dir / 'dbus-1'
dbus_services_dir = dbus_dir / 'services'
desktop_dir = data_dir / 'applications'

systemd_user_unit_dir = get_option('systemd_user_unit_dir')
systemd_dep = dependency('systemd', version: '>= 242', required: false)
if systemd_user_unit_dir == ''
  if systemd_dep.found()
    systemd_user_unit_dir = systemd_dep.get_variable(
      pkgconfig: 'systemduserunitdir',
    )
  else
    systemd_user_unit_dir = prefix / lib_dir / 'systemd' / 'user'
  endif
endif

cargo = find_program('cargo')
cargo_args = ['--manifest-path', meson.project_source_root() / 'Cargo.toml']
cargo_args += ['--target-dir', meson.project_build_root() / 'target']

if get_option('debug')
  cargo_target = 'debug'
else
  cargo_args += ['--release']
  cargo_target = 'release'
endif

cargo_env = environment()
cargo_env.set('CARGO_HOME', meson.project_build_root() / 'cargo')

subdir('po')
subdir('data')
subdir('phosh-os-updater')
subdir('phosh-syncbus')

if get_option('demo')
  subdir('demo')
endif

gnome.post_install(gtk_update_icon_cache: true, update_desktop_database: true)
