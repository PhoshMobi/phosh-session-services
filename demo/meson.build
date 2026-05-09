libadw_dep = dependency('libadwaita-1', version: '>= 1.8', required: true)

exe_target = custom_target(
    demo_exe_name,
    build_by_default: true,
    build_always_stale: true,
    console: true,
    install: true,
    install_dir: libexec_dir,
    env: cargo_env,
    output: demo_exe_name,
    command: [
        cargo,
        'build',
        cargo_args + ['-p', 'demo'],
        '&&',
        'cp',
        meson.project_build_root() / 'target' / cargo_target / 'demo',
        '@OUTPUT@',
    ],
)
