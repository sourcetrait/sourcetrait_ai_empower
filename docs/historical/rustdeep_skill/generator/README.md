
The `GENERATE_RUSTDEEP_SKILL.md` is prompt text that takes two sources of input:
- `./input` Layered prompting on how to author the skill (required)
- `./compare` The original generated skill to compare output against (optional, but best to include)

The prompting calls the skill that it generates `repo-orientation`, not `rustdeep`.

The skill that this prompt generates (including Python scripts) is intended to,
in turn, build a deep-read orientation skill for a specific Rust repository
specified by the user.

Its output is a skill to build another repo-specific skill. This prompt is a
prompt to build a skill that builds a skill. Confused yet?

A specific repository's orientation skill can then be loaded before
development is to begin, giving it an advantage over having to cold-read
the entire repository.