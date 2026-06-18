use crate::*;

// The following field naming conventions for the `shm` paths in consumers:
// - `shm_ident`: The name of an AI harness
//    - Corresponds with ~/ai/<ident>/
//    - Corresponds with {SHM_BOX_AI_DIR}/<ident>/
//    - Corresponds with ~/tmp/ai/<ident>/
// - `shm_unique`: Unique token (base62 if generated here, a uncommon snake if authored by the agent)
//   - Note: Claude's Write tool performs all mkdirs, so it doesn't need to use a shell tool to write an IPC shm file
// - `shm_dir`: {SHM_BOX_AI_DIR}/<ident>/<unique>/ Transitive location of IPC files, typically one-turn or one batch of turns.
//   - SHM_BOX_AI_DIR is convention, so only "ident/unique" needs to be passed to the MPC; where the code can split on '/' to extract both tokens
// - `shm_file`: {SHM_BOX_AI_DIR}/<ident>/<unique>/<filename> 
//   - SHM_BOX_AI_DIR is convention, so only "ident/unique/filename" needs to be passed to the MPC; where the code can split on '/' to extract the three
// - `shm_files`: Used if `shm_dir` (or its components) is already being passed to the MPC and there are multiple files; filename only here.
// 
// When the agent is making a MCP run/call/interact, args fields need to send EITHER:
// - shm_ident, shm_unique, and a list of shm_file
// - a path relative to {SHM_BOX_AI_DIR}

pub(crate) const SHM_BOX_AI_DIR: &'static str = "/dev/shm/box/ai";