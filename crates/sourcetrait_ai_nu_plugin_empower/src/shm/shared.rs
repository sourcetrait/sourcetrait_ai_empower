//! Relevant empower gaurantees:
//! - `$env.XDGX_SHM_DIR` exists; set to `/dev/shm/($env.USER)` (/dev/shm/box)
//! 
//! Field naming conventions as passed as arguments / parameters by the agent:
//! - `shm_author: path` := Categorizes "who" authored a path; (a/b) or (a/b/c)
//!    - An agent? An MCP library mod? An MCP run/interact?
//!    - The intention here is to generally categorize origin. It shouldn't be
//!      verbose. Typically, authors (agents / source-code) should prefer 2 path
//!      components.
//!    - Agents: `ai/<fae_name>`. eg. `ai/billy_bob`
//!    - Nushell MCP run/interact: `mcp/run` and `mcp/interact`
//!    - MCP libraries (including calls): `<library>/<categorical/mod/path>`.
//!      eg `empower/know` or `empower/git` for all of their sub-modules
//!    - etc ...
//! - `shm: path` := `($env.XDGX_SHM_DIR)/($shm)` Transitive relative path for
//!   IPC file.
//!   - Coming from the agent, this is `<shm_author>/<filename>`
//! - `shm_files: record<dir: path, names: list<string>`
//!   := `($env.XDGX_SHM_DIR/($shm_files.dir)/[..$shm_files.names])`
//!   Transitive list of IPC filenames within a IPC directory.
//!   - Coming from the agent, dir is `<shm_author>` and filenames are relative
//!     to that.
//! - `shm_dir: path` := `($env.XDGX_SHM_DIR)/($shm_dir)` Transitive relative
//!   path for IPC directory.
//!   - Coming from the agent, this is usually `<shm_author>`

//use crate::*;

