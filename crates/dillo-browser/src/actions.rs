//! Port de `src/actions.c` + `actions.h` — actions utilisateur
//! (`label: commande`) lancées sur les liens et la page.
//!
//! Les listes viennent de `prefs.link_actions`/`page_actions`
//! (parser `strtok` amont : label = jusqu'au premier `:`, cmd = tout le
//! reste). `a_Actions_run` = `fork` + `system(cmd)` dans l'enfant avec
//! `DILLO_PID` — ici un `Command` shell détaché (le parent ne wait pas,
//! comme amont).

use std::process::Command;

/// `Action` — `{label, cmd}`.
pub struct Action {
    /// `action->label`.
    pub label: String,
    /// `action->cmd`.
    pub cmd: String,
}

/// `action_parse` — `strtok(line, ":")` puis `strtok(NULL, "")`.
///
/// Retourne `None` si le label ou la commande manque (MSG amont → log
/// injecté côté appelant ; ici `None` suffit, l'appelant sait pourquoi).
fn action_parse(line: &str) -> Option<Action> {
    // label = jusqu'au premier ':' (strtok sauterait les vides initiaux
    // — fidèle : `label == NULL || strlen == 0`).
    let (label, cmd) = line.split_once(':')?;
    if label.is_empty() {
        return None;
    }
    // `strtok(NULL, "")` retourne le reste tel quel (délimiteur vide).
    if cmd.is_empty() {
        return None;
    }
    Some(Action {
        label: label.to_string(),
        cmd: cmd.to_string(),
    })
}

/// `actions_parse` — parse chaque ligne de `pref_actions` dans l'ordre
/// (les prefs stockent des octets bruts — `char*` amont ; les lignes
/// non-UTF-8 sont décodées lossy comme partout dans le port).
#[must_use]
pub fn actions_parse(pref_actions: &[Vec<u8>]) -> Vec<Action> {
    pref_actions
        .iter()
        .filter_map(|l| action_parse(&String::from_utf8_lossy(l)))
        .collect()
}

/// `a_Actions_init` — les deux listes issues des prefs.
#[must_use]
pub fn actions_init(prefs: &crate::prefs::Prefs) -> (Vec<Action>, Vec<Action>) {
    (
        actions_parse(&prefs.link_actions),
        actions_parse(&prefs.page_actions),
    )
}

/// `a_Actions_run` — fork + `system(cmd)` dans l'enfant avec
/// `DILLO_PID` = pid de dillo ; le parent retourne immédiatement.
///
/// Ici : `Command` détaché (pas de `wait`) avec `DILLO_PID` — même
/// sémantique observable (l'enfant exécute le shell et se termine).
///
/// # Errors
/// Retourne `Err` si le spawn échoue (≙ `fork == -1` amont).
pub fn actions_run(action: &Action) -> std::io::Result<()> {
    let pid = std::process::id().to_string();
    let mut cmd = shell_command(&action.cmd);
    cmd.env("DILLO_PID", pid);
    cmd.spawn().map(|_child| ())
}

#[cfg(unix)]
fn shell_command(cmd: &str) -> Command {
    let mut c = Command::new("sh");
    c.arg("-c").arg(cmd);
    c
}

#[cfg(windows)]
fn shell_command(cmd: &str) -> Command {
    let mut c = Command::new("cmd");
    c.arg("/C").arg(cmd);
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_label_cmd() {
        let a = action_parse("open:xdg-open %s").unwrap();
        assert_eq!(a.label, "open");
        assert_eq!(a.cmd, "xdg-open %s");
        // `strtok(NULL, "")` : le reste garde les ':' ultérieurs.
        let a = action_parse("u:echo a:b:c").unwrap();
        assert_eq!(a.cmd, "echo a:b:c");
        // label ou cmd vide → ignoré.
        assert!(action_parse(":cmd").is_none());
        assert!(action_parse("label:").is_none());
        assert!(action_parse("no-colon").is_none());
    }
}
