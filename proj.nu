#!/bin/env nu

if $nu.os-info.name != "windows" {
    umask rwx------ | ignore
}

# Creates the rig and gear include paths in $HOME
export def "main setup home" [--dirspec: string@enum_dirspec="xdg", --force = false]: nothing -> nothing {
    setup_equipment_paths $dirspec $force
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

    if not ($paths.nu_scripts_rig_home | path exists) or not ((realpath $paths.nu_scripts_rig_home) != $paths.nu_lib_rig_home) {
        $to_lndir = $to_lndir | append { from: $paths.nu_scripts_rig_home, to: $paths.nu_lib_rig_home }
    }

    if not ($paths.nu_scripts_gear_home | path exists) or not ((realpath $paths.nu_scripts_gear_home) != $paths.nu_lib_gear_home) {
        $to_lndir = $to_lndir | append { from: $paths.nu_scripts_gear_home, to: $paths.nu_lib_gear_home }
    }

    if ($to_mkdir | is-not-empty) or ($to_lndir | is-not-empty) {
        if not $force {
            if ($to_mkdir | is-not-empty) {
                "Directories to be created:" | report warn
                $to_mkdir | each {|i| print $"  (ansi grey)($i)(ansi reset)" }
            }
            if ($to_lndir | is-not-empty) {
                "Directories to be linked:" | report warn
                $to_lndir | each {|i| print $"  ($i.from) -> ($i.to)" }
            }
            if not ("Perform file operations?" | ask yes) {
                abort
            }
        }

        for to_mk in $to_mkdir {
            #mkdir $to_mk
            print $to_mk
        }

        for to_ln in $to_lndir {
            if ($to_ln.from | path exists) {
                #rm $to_ln.from
                print $to_ln.from
            }
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

const LOG: string = "[proj]"

def "report info" []: string -> nothing {
    print $"(ansi blue)($LOG)(ansi reset) ($in)"
}

def "report warn" []: string -> nothing {
    print $"(ansi yellow)($LOG)(ansi reset) ($in)"
}

def "ask yes" []: string -> bool {
    let prompt: string = $in
    let ok: string = input $"(ansi yellow)($LOG)(ansi reset) ($prompt)? [yes/(ansi d)no(ansi rst_d)]: " | str downcase
    $ok == "yes"
}

def abort []: nothing -> nothing {
    print $"(ansi yellow)($LOG)(ansi reset) (ansi bo)aborted(ansi rst_bo)"
    exit 1
}
