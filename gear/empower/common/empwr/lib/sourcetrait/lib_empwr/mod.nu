# Internal common library for the empwr gear (gear/common/sourcetrait/lib_empwr).
#
# Not imported by users directly: the desk empwr library re-exports its
# modules and the box empwr bin fronts them. Carries the target-neutral
# machinery both sides share - currently the `mcp` wrapper module.

export module mcp
