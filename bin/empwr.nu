#!/bin/env nu

export def "main ant colony new" [fae: string]: nothing -> nothing {
    let ant_dir = (pwd)
    if not ($ant_dir | path exists) {
        error make $"Directory is does not exist: ($ant_dir)"
    }

    check_ant_repo
    cd (^git rev-parse --show-toplevel)

    let colony_branch = $"colony/($fae)"
    if ($colony_branch in git_branches) {
        error make $"Colony branch already exists: ($colony_branch)"
    }

    ^git branch $colony_branch template/colony/default
    ^git worktree add $colony_branch $colony_branch    
    cd $colony_branch
    open --raw config/queen.yaml.template
        | templation [[ai_identity $"ant_($fae)"] [bonded_fae_identity $fae]]
        | save config/queen.yaml
    rm config/queen.yaml.template
    ^git add .
    ^git commit -m$"init ($colony_branch)"
}

def templation [fill: list<list<string>>]: string -> string {
    mut s = $in
    for pair in $fill {
        $s = $s | str replace -r $"%{($pair.0)}%" $pair.1
    }

    $s
}

# Creates a new ant harness repository, which is a collection of colonies.
export def "main ant new" [ant_dir: directory, harness_templates_dir?: directory]: nothing -> nothing {
    if (($ant_dir | path exists) and (ls -a $ant_dir | is-not-empty)) {
        error make $"Directory is NOT empty: ($ant_dir)"
    }
    let harness_templates_dir = if ($harness_templates_dir == null) {
        find_harness_templates_dir
    } else if not ($harness_templates_dir | path exists) {
        error make "Empower harness template directory not found"
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

def find_harness_templates_dir []: nothing -> oneof<directory, error> {
    let dir: directory = ($env.HOME | path join 'proj/sourcetrait/sourcetrait_ai_empower/harness')
    if ($dir | path exists) { return $dir }

    let dir: directory = /usr/local/share/sourcetrait/empower/harness
    if ($dir | path exists) { return $dir }

    error make "Empower harness template directory not found"
}

def check_ant_repo []: nothing -> oneof<nothing, error> {
    let branches: list<string> = (^git for-each-ref --format='%(refname:short)' refs/heads/)
    if not ('ant' in $branches) { error make "Ant repository is missing branch: ant" }
    if not ('template/colony/default' in $branches) { error make "Ant repository is missing branch: template/colony/default" }
    if ('ant' != (^git branch --show-current)) { error make "Not currently in 'ant' branch" }
    return
}

def git_branches []: nothing -> list<string> {
    ^git for-each-ref --format='%(refname:short)' refs/heads/
}

export def "main ant colony" [] { help main ant colony }
export def "main ant" [] { help main ant }
export def main [] { help main }
