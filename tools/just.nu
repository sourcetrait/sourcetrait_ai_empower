#!/bin/env nu

if $nu.os-info.name != "windows" {
    umask rwx------ | ignore
}

export def "main install" []: nothing -> nothing {
    setup_equipment_home
}

export def main []: nothing -> nothing { help main }

def setup_equipment_home []: nothing -> nothing {
    let paths = equip_paths

    if not ($paths.nu_scripts_home | path exists) {
        mkdir $paths.nu_scripts_dir
    }

    if not ($paths.nu_lib_home | path exists) {
        mkdir $paths.nu_lib_dir
    }

    if not ($paths.nu_lib_rig_home | path exists) {
        mkdir $paths.nu_lib_rig_dir
    }

    if not ($paths.nu_lib_gear_home | path exists) {
        mkdir $paths.nu_lib_gear_dir
    }

    if not ($paths.nu_scripts_rig_home | path exists) {
        linkdir $paths.nu_lib_rig_dir $paths.nu_scripts_rig_dir
    }

    if not ($paths.nu_scripts_gear_home | path exists) {
        linkdir $paths.nu_lib_gear_dir $paths.nu_scripts_gear_dir
    }
}

def linkdir [from: directory, to: directory]: nothing -> nothing {
    match $nu.os-info.name {
        "windows" => { ^mklink /D $from $to }
        _ => { ^ln -s $from $to }
    }
}

def equipment_paths []: nothing -> record<config_home: directory, library_home: directory, nu_lib_home: directory, nu_lib_gear_home: directory, nu_lib_rig_home: directory, nu_scripts_home: directory, nu_scripts_gear_home: directory, nu_scripts_rig_home: directory> {
    let config_home = ($env | get -o XDG_CONFIG_HOME | default ($env.HOME | path join '.config'))
    let library_home = ($env | get -o XDGX_LIBRARY_HOME | default ($env.HOME | path join '.sys/local/lib'))
    let nu_lib_home = $library_home | path join 'nu'
    let nu_lib_gear_home = $nu_lib_home | path join 'gear'
    let nu_lib_rig_home = $nu_lib_home | path join 'rig'
    let nu_scripts_home = $config_home | path join 'nushell/scripts'
    let nu_scripts_gear_home = $nu_scripts_home | path join 'gear'
    let nu_scripts_rig_home = $nu_scripts_home | path join 'rig'

    {
        config_home: $config_home,
        library_home: $library_home,
        nu_lib_home: $nu_lib_home,
        nu_lib_gear_home: $nu_lib_gear_home,
        nu_lib_rig_home: $nu_lib_rig_home,
        nu_scripts_home: $nu_scripts_home
        nu_scripts_gear_home: $nu_scripts_gear_home
        nu_scripts_rig_home: $nu_scripts_rig_home
    }
}
