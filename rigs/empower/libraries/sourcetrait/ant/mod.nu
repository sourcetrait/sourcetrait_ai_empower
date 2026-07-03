# Shared substrate for the ant colony roles (sourcetrait/ant).
#
# Relatively empty by design: most colony functionality is role-specific and
# lives in sourcetrait/queen / sourcetrait/drone; this library carries only
# what both roles share. See ant:channel.
export module channel
