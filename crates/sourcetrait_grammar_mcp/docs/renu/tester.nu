#!/usr/bin/nu

use demo
let result_demo_attrs = demo demo_attrs shm 5
let result_help_find_demo_attrs = help --find 'demo::category' | select name search_terms
let result_scope_demo_attrs = scope commands | where name == 'demo demo_attrs' | first

let record = {
    results: {
        demo_attrs: $result_demo_attrs
        help_find_demo_attrs: $result_help_find_demo_attrs
        scope_demo_attrs: $result_scope_demo_attrs
    }
}

match ($env | get -o TONUON | default false | into bool) {
    true => ($record | to nuon)
    false => $record
}