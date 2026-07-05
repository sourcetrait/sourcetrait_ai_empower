#!/bin/env nu

if $nu.os-info.name != "windows" {
    umask rwx------ | ignore
}

export def "main setup home" [--spec: string@enum_spec="xdg", --force = false]: nothing -> nothing {
    setup_equipment_home $spec $force
}

export def "main setup" []: nothing -> nothing { help main setup }

export def main []: nothing -> nothing { help main }

def enum_spec []: nothing -> list<string> {
    [ xdg box ]
}

def setup_equipment_home [spec: string@enum_spec = "xdg", force: bool = false]: nothing -> nothing {
    let paths = equipment_paths $spec

    mut to_mkdir: list<directory> = []
    mut to_lndir: table<from: directory, to: directory> = []
    
    if not ($paths.nu_lib_home | path exists) {
        $to_mkdir = $to_mkdir | append $paths.nu_lib_home
    }

    if not ($paths.nu_lib_rig_home | path exists) {
        $to_mkdir = $to_mkdir | append $paths.nu_lib_rig_home
    }

    if not ($paths.nu_lib_gear_home | path exists) {
        $to_mkdir = $to_mkdir | append $paths.nu_lib_gear_home
    }

    if not ($paths.nu_scripts_home | path exists) {
        $to_mkdir = $to_mkdir | append $paths.nu_scripts_dir
    }

    if not ($paths.nu_scripts_rig_home | path exists) {
        $to_lndir = $to_lndir | append $paths.nu_scripts_rig_home
    }

    if not ($paths.nu_scripts_gear_home | path exists) {
        $to_lndir = $to_lndir | append $paths.nu_scripts_gear_home
    }

    if ($to_mkdir | is-not-empty) or ($to_lndir | is-not-empty) {
        if not $force {
            if ($to_mkdir | is-not-empty) {
                print $"(ansi yellow)[just.nu](ansi reset) Directories to be created:"
                print $to_mkdir
            }
            if ($to_lndir | is-not-empty) {
                print $"(ansi yellow)[just.nu](ansi reset) Directories to be linked:"
                print $to_lndir
            }
            let ok = input $"(ansi yellow)[just.nu](ansi reset) Perform operations? [yes/(ansi grey)no(ansi reset)]: "
            if $ok != "yes" {
                print $"(ansi red)[just.nu](ansi reset) Aborted"
                exit 1
            }
        }

        for to_mk in $to_mkdir {
            #mkdir $to_mk
            print $to_mk
        }

        for to_ln in $to_lndir {
            #linkdir $to_ln.from $to_ln.to
            print $to_ln
        }
    }
}

def linkdir [from: directory, to: directory]: nothing -> nothing {
    match $nu.os-info.name {
        "windows" => { ^mklink /D $from $to }
        _ => { ^ln -s $from $to }
    }
}

def equipment_paths [spec: string@enum_spec="xdg"]: nothing -> record<config_home: directory, library_home: directory, nu_lib_home: directory, nu_lib_gear_home: directory, nu_lib_rig_home: directory, nu_scripts_home: directory, nu_scripts_gear_home: directory, nu_scripts_rig_home: directory> {
    let config_home = ($env | get -o XDG_CONFIG_HOME | default ($env.HOME | path join '.config'))
    let library_home = ($env | get -o XDGX_LIBRARY_HOME | default (match $spec {
        "xdg" => ($env.HOME | path join '.local/lib')
        "box" => ($env.HOME | path join '.sys/local/lib')
    }))
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
