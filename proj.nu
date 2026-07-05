#!/bin/env nu

if $nu.os-info.name != "windows" {
    umask rwx------ | ignore
}

use ./tools/nu/tooling *
const WHO: string = "proj"

# Creates the rig and gear include paths in $HOME
export def "main setup home" [--dirspec: string@enum_dirspec="xdg", --force = false]: nothing -> nothing {
    "Setting up home ..." | report info $WHO
    setup_equipment_paths $dirspec $force
    "Done setting up home" | report ok $WHO
}

export def "main setup" []: nothing -> nothing { help main setup }

export def main []: nothing -> nothing { help main }

def enum_dirspec []: nothing -> list<string> {
    [ xdg dotsys ]
}

def setup_equipment_paths [spec: string@enum_dirspec = "xdg", force: bool = false]: nothing -> nothing {
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

    if not ($paths.nu_scripts_rig_home | path exists) or not ((realpath $paths.nu_scripts_rig_home) == $paths.nu_lib_rig_home) {
        $to_lndir = $to_lndir | append { from: $paths.nu_scripts_rig_home, to: $paths.nu_lib_rig_home }
    }

    if not ($paths.nu_scripts_gear_home | path exists) or not ((realpath $paths.nu_scripts_gear_home) == $paths.nu_lib_gear_home) {
        $to_lndir = $to_lndir | append { from: $paths.nu_scripts_gear_home, to: $paths.nu_lib_gear_home }
    }

    if ($to_mkdir | is-not-empty) or ($to_lndir | is-not-empty) {
        if not $force {
            if ($to_mkdir | is-not-empty) {
                "Directories to be created:" | report warn $WHO
                $to_mkdir | each {|i| print $"  (ansi grey)($i)(ansi reset)" }
            }
            if ($to_lndir | is-not-empty) {
                "Directories to be linked:" | report warn $WHO
                $to_lndir | each {|i| print $"  (ansi grey)($i.from)(ansi reset) to (ansi grey)($i.to)(ansi reset)" }
            }
            if not ("Perform file operations?" | ask yes $WHO) {
                abort $WHO
            }
        }

        for to_mk in $to_mkdir {
            mkdir $to_mk
        }

        for to_ln in $to_lndir {
            if ($to_ln.from | path exists) {
                rm $to_ln.from
            }

            linkdir $to_ln.from $to_ln.to
        }
    }
}

def equipment_paths [dirspec: string@enum_dirspec="xdg"]: nothing -> record<config_home: directory, library_home: directory, nu_lib_home: directory, nu_lib_gear_home: directory, nu_lib_rig_home: directory, nu_scripts_home: directory, nu_scripts_gear_home: directory, nu_scripts_rig_home: directory> {
    let config_home = ($env | get -o XDG_CONFIG_HOME | default ($env.HOME | path join '.config'))
    let library_home = ($env | get -o XDGX_LIBRARY_HOME | default (match $dirspec {
        "xdg" => ($env.HOME | path join '.local/lib')
        "dotsys" => ($env.HOME | path join '.sys/local/lib')
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

