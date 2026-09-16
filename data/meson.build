subdir('icons')

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

desktop_file_in = 'mobi.phosh.service.desktop.in'
service_file_in = 'phosh-service.service.in'
desktop_utils = find_program('desktop-file-validate', required: false)
foreach service : services
  service_exe = service[0]
  app_id = service[1]
  name = service[2]

  desktop_conf = configuration_data()
  desktop_conf.set('libexecdir', libexec_dir)
  desktop_conf.set('service_exe', service_exe)
  desktop_conf.set('icon', app_id)
  desktop_conf.set('name', name)

  desktop_file = configure_file(
    input: desktop_file_in,
    output: f'@app_id@.desktop',
    configuration: desktop_conf,
    install_dir: desktop_dir,
  )

  # TODO: Phosh is allowd in OnlyShowIn but desktop-file-utils didn't have a
  # release yet
  #if desktop_utils.found()
  #    test('Validate desktop file', desktop_utils, args: [desktop_file])
  #endif

  service_conf = configuration_data()
  service_conf.set('libexecdir', libexec_dir)
  service_conf.set('description', name)
  service_conf.set('service_exe', service_exe)
  service_conf.set('name', name)

  service_file = configure_file(
    input: service_file_in,
    output: f'@service_exe@.service',
    configuration: service_conf,
    install_dir: systemd_user_unit_dir,
  )
endforeach
