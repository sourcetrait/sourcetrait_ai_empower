use crate::*;

/// The grammar cert profile shipped for the operator to run srcert against.
const GRAMMAR_PROFILE: &str = include_str!("../../defaults/grammar_profile.toml");

/// Ship the grammar cert profile when absent; ship-and-continue, never fatal.
pub(crate) fn ensure_cert_profile() {
    let config_home = match lib_cert::config_home() {
        Ok(home) => home,
        Err(e) => {
            eprintln!("grammar: cert profile not shipped: {e}");
            return;
        }
    };
    match lib_cert::install_profile(&config_home, lib_grammar::consts::GRAMMAR, GRAMMAR_PROFILE) {
        Ok(done) if done.written => eprintln!(
            "grammar: shipped the `{name}` cert profile to {path}; run \
             `srcert generate {name} <dir>` then `srcert install {name} <dir>` \
             to enable the channel",
            name = lib_grammar::consts::GRAMMAR,
            path = done.live.display(),
        ),
        Ok(_) => {}
        Err(e) => eprintln!("grammar: cert profile not shipped: {e}"),
    }
}
