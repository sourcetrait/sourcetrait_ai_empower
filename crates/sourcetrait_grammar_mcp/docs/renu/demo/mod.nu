export alias "attr myattr" = echo

# This is a summary line.
#
# These are details.
# @notattr no workie
@category demo
@search-terms 'demo::time'
@example 'tests shm' {
  demo_attrs shm 6
} --result { kind: shm, some: 6 }
@example 'tests tmp' {
  demo_attrs tmp 7
} --result { kind: tmp, some: 7 }
@myattr foo 8 'Something flies here' {k: 'keyed', v: 'valued'} [[field_a field_b]; [hey 1] [there 2]]
export def demo_attrs [
  kind: string@[shm tmp] # The kind
  some: int # The some
]: nothing -> record<kind: string, some: int> {
  { kind: $kind, some: $some }
}
