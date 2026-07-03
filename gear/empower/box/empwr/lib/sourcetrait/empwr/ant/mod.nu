# Box-side ant provisioning: create ant repositories and colony worktrees.
# Ported verbatim from the retired empwr bin's `main ant *` commands (the
# `error make` calls gained their required record shape in the port).

# Creates a new ant harness repository, which is a collection of colonies.
export def new [ant_dir: directory, harness_templates_dir?: directory]: nothing -> nothing {
    if (($ant_dir | path exists) and (ls -a $ant_dir | is-not-empty)) {
        error make {msg: $"Directory is not empty: ($ant_dir)"}
    }
    let harness_templates_dir = if ($harness_templates_dir == null) {
        find_harness_templates_dir
    } else if not ($harness_templates_dir | path exists) {
        error make {msg: "Empower harness template directory not found"}
    } else {
        $harness_templates_dir
    }

    mkdir $ant_dir
    cd $ant_dir
    ^git init -b ant .
    touch .gitignore
    ^git add .gitignore
    ^git commit -m'init'
    ^git branch template/init
    ^git branch template/colony/default
    ^git switch template/colony/default
    ^rsync -a ($harness_templates_dir)/ant/colony/ .
    ^git add .
    ^git commit -m'init template'
    ^git switch ant
    '/colony' | save --append .gitignore
    ^git add .
    ^git commit -m'init ant'
}

export def "colony new" [fae: string]: nothing -> nothing {
    let ant_dir = (pwd)
    if not ($ant_dir | path exists) {
        error make {msg: $"Directory does not exist: ($ant_dir)"}
    }

    check_ant_repo
    cd (^git rev-parse --show-toplevel)

    let colony_branch = $"colony/($fae)"
    if ($colony_branch in (git_branches)) {
        error make {msg: $"Colony branch already exists: ($colony_branch)"}
    } else if (pwd | path join $colony_branch | path exists) {
        error make {msg: $"Colony directory already exists: ($colony_branch)"}
    }

    let colony_claude_project_dir = (
        $env.CLAUDE_CONFIG_DIR | path join 'projects'
        | path join (pwd | path join $colony_branch | slugify path)
    )
    if ($colony_claude_project_dir | path exists) {
        $"Previous claude project exists: (ansi cyan)($colony_claude_project_dir)(ansi reset)" | report warn
        let yn: string = input $"(ansi red_bold)Retire(ansi reset) previous claude project? [yes/(ansi d)no(ansi rst_d)]: " | str downcase
        if ($yn != 'yes') {
            $"(ansi bo)aborted(ansi rst_bo)" | report warn
            return
        }

        retire dir $colony_claude_project_dir ($env.HOME | path join 'tmp/retired/claude/projects')
            | do { $"Retired to: (ansi cyan)($in)(ansi reset)" }
            | report info
    }

    ^git branch $colony_branch template/colony/default
    ^git worktree add $colony_branch $colony_branch
    cd $colony_branch

    open --raw queen/config/queen.yaml.template
        | templation [[ai_identity $"ant_($fae)"] [fae_identity $fae]]
        | save queen/config/queen.yaml
    rm queen/config/queen.yaml.template

    open --raw drone/config/drone.yaml.template
        | templation [[ai_identity $"ant_($fae)"] [fae_identity $fae]]
        | save drone/config/drone.yaml
    rm drone/config/drone.yaml.template

    ^git add .
    ^git commit -m$"init ($colony_branch)"

    $"(ansi green)done(ansi reset)" | report info
}

def "report info" []: string -> nothing {
    print $"(ansi blue)[empwr](ansi reset) ($in)"
}

def "report warn" []: string -> nothing {
    print $"(ansi yellow)[empwr](ansi reset) ($in)"
}

def "retire dir" [dir: directory, to: directory]: nothing -> directory {
    mut retired_to: oneof<directory, nothing> = null
    while $retired_to == null {
        let to_rando: directory = $to | path join (random chars -l 4)
        if ($to_rando | path exists) { continue }
        mkdir $to_rando
        mv $dir $to_rando
        $retired_to = $to_rando | path join ($dir | path basename)
    }

    $retired_to
}

def templation [fill: list<list<string>>]: string -> string {
    mut s = $in
    for pair in $fill {
        $s = $s | str replace -r $"%{($pair.0)}%" $pair.1
    }

    $s
}

def "slugify path" []: string -> string {
    $in | str replace --all --regex '[^a-zA-Z0-9]' '-'
}

def find_harness_templates_dir []: nothing -> oneof<directory, error> {
    let dir: directory = ($env.HOME | path join 'proj/sourcetrait/sourcetrait_ai_empower/harness')
    if ($dir | path exists) { return $dir }

    let dir: directory = /usr/local/share/sourcetrait/empower/harness
    if ($dir | path exists) { return $dir }

    error make {msg: "Empower harness template directory not found"}
}

def check_ant_repo []: nothing -> oneof<nothing, error> {
    let branches: list<string> = (^git for-each-ref --format='%(refname:short)' refs/heads/)
    if not ('ant' in $branches) { error make {msg: "Ant repository is missing branch: ant"} }
    if not ('template/colony/default' in $branches) { error make {msg: "Ant repository is missing branch: template/colony/default"} }
    if ('ant' != (^git branch --show-current)) { error make {msg: "Not currently in 'ant' branch"} }
    return
}

def git_branches []: nothing -> list<string> {
    ^git for-each-ref --format='%(refname:short)' refs/heads/
}
