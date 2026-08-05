#!/usr/bin/nu

use demo
let result_demo_attrs = demo demo_attrs shm 5
let result_help_find_demo_attrs = help --find 'demo::time' | select name search_terms
let result_scope_demo_attrs = scope commands | where name == 'demo demo_attrs' | first

{
    results: {
        demo_attrs: $result_demo_attrs
        help_find_demo_attrs: $result_help_find_demo_attrs
        scope_demo_attrs: $result_scope_demo_attrs
    }
}

