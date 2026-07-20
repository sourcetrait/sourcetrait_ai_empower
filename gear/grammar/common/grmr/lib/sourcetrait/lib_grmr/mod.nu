# Internal common library for the grmr gear (gear/common/sourcetrait/lib_grmr).
#
# Not imported by users directly: the desk grmr library re-exports its
# modules and the box grmr bin fronts them. Carries the target-neutral
# machinery both sides share - currently the `mcp` wrapper module.

export module mcp
