conf_data = configuration_data()
conf_data.set('LIBEXEC_DIR', libexec_dir)
conf_data.set('EXE_NAME', server_exe_name)

configure_file(
    input: 'mobi.phosh.syncbus.service.in',
    output: 'mobi.phosh.syncbus.service',
    configuration: conf_data,
    install_dir: dbus_services_dir,
)

configure_file(
    input: 'phosh-syncbus.service.in',
    output: 'phosh-syncbus.service',
    configuration: conf_data,
    install_dir: systemd_user_unit_dir,
)

install_subdir('interfaces', install_dir: dbus_dir)
