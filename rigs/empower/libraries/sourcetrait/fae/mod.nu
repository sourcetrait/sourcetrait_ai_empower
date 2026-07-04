# Fae role library (sourcetrait/fae).
#
# Tooling addressing the bonded colony (queen and drones) lives under `colony`;
# the fae's memory knowledge-base ops under `memory`; the knowledge authoring
# pipeline under `know`; role asset-tree helpers under `fs`; skeleton
# generation under `soak`. Self-contained by design: no fae<->ant/queen/drone
# dependencies (shared-looking derivations are deliberately duplicated per
# side).
export module asset 
export module colony
export module layout 
export module soak
export module memory
export module know
