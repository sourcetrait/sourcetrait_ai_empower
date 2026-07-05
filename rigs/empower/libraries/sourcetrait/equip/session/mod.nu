# Session settings 

# > HUMAN

export def equip_id []: nothing -> string {
    $env | get -o EQUIP_ID | default $env.USER
}

export def equip_namespace []: nothing -> string {
    $env | get -o EQUIP_NAMESPACE | default 'default'
}

export def equip_work_dir []: nothing -> string {
    $env | get -o EQUIP_WORK_DIR | default ($env.HOME | path join 'proj/equip' (equip_id) (equip_namespace))
}