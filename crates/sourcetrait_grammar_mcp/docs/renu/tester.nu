#!/usr/bin/nu

glob ** | each {|f|
  if (($f | path type) == 'file') {
    print $"# FILE ($f):"
    open --raw $f | print
    print "## EOF\n\n"
  }
}

print "# OUTPUT (tester.nu):"
use demo
demo demo_attrs shm 5 | to nuon | print
help --find 'demo::time' | select name search_terms | to nuon | print
scope commands | where name == 'demo demo_attrs' | first | to nuon --pretty | print


