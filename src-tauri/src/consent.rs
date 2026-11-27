//! A phone action commits only after the user agrees.
//!
//! The model can set `confirmed: true` by itself. That flag counts only when a
//! user speech turn or a confirm tap landed after the question, and within 30 seconds.

use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const CONFIRM_TTL: Duration = Duration::from_secs(30);

struct Stage {
    key: String,
    seq: u64,
    at: Instant,
}

struct State {
    seq: u64,
    text: String,
    turn_at: Instant,
    tap: bool,
    stage: Option<Stage>,
}

impl State {
    fn new() -> Self {
        Self {
            seq: 0,
            text: String::new(),
            turn_at: Instant::now(),
            tap: false,
            stage: None,
        }
    }
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(State::new()));

fn lock() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|err| err.into_inner())
}

/// Record a typed or transcribed user turn. Assistant text must not come here.
pub fn note_user_turn(text: &str) {
    let mut state = lock();
    state.seq = state.seq.wrapping_add(1);
    state.text = text.to_string();
    state.turn_at = Instant::now();
    state.tap = false;
}

/// The confirm button. It agrees to the action that is already staged.
pub fn note_tap() {
    let mut state = lock();
    state.seq = state.seq.wrapping_add(1);
    state.turn_at = Instant::now();
    state.tap = true;
}

/// Remember the question. A repeat of the same question keeps the original clock.
pub fn stage(key: &str) {
    let mut state = lock();
    let keep = state
        .stage
        .as_ref()
        .is_some_and(|stage| stage.key == key && stage.at.elapsed() <= CONFIRM_TTL);
    if keep {
        return;
    }
    state.stage = Some(Stage { key: key.to_string(), seq: state.seq, at: Instant::now() });
    state.tap = false;
}

/// True when this exact action was staged and the user agreed afterwards.
pub fn try_consume(key: &str) -> bool {
    let mut state = lock();
    let ready = state.stage.as_ref().is_some_and(|stage| {
        stage.key == key
            && stage.at.elapsed() <= CONFIRM_TTL
            && state.seq > stage.seq
            && state.turn_at.elapsed() <= CONFIRM_TTL
            && (state.tap || affirmative(&state.text))
    });
    if ready {
        state.stage = None;
        state.tap = false;
    }
    ready
}

pub fn affirmative(text: &str) -> bool {
    let tokens = tokens_of(text);
    if tokens.is_empty() || tokens.iter().any(|token| NO.contains(&token.as_str())) {
        return false;
    }
    if tokens.iter().any(|token| YES.contains(&token.as_str())) {
        return true;
    }
    let joined = tokens.join(" ");
    PHRASES.iter().any(|phrase| joined.contains(phrase))
}

fn tokens_of(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                current.push(lower);
            }
        } else if ch == '\'' || ch == '’' {
            // d'acord and don't stay one token.
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

const YES: &[&str] = &[
    "yes", "yeah", "yep", "yup", "ok", "okay", "sure", "confirm", "confirmed", "alright",
    "si", "sí", "vale", "claro", "confirmo", "adelante",
    "ja", "klar", "genau", "einverstanden",
    "sim", "pode",
    "val", "clar", "confirma", "dacord",
];

const NO: &[&str] = &[
    "no", "nope", "nah", "not", "dont", "never", "cancel", "stop", "wait",
    "nunca", "cancela", "cancelar", "espera",
    "nein", "nicht", "abbrechen", "warte", "stopp",
    "não", "nao",
    "mai",
];

const PHRASES: &[&str] = &[
    "go ahead",
    "do it",
    "de acuerdo",
    "all right",
    "mach das",
    "pode ser",
    "fes ho",
];

#[cfg(test)]
pub fn hold_for_test() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(|err| err.into_inner());
    reset_for_test();
    guard
}

#[cfg(test)]
pub fn reset_for_test() {
    *lock() = State::new();
}

#[cfg(test)]
pub fn expire_for_test() {
    let mut state = lock();
    let old = Instant::now() - Duration::from_secs(31);
    if let Some(stage) = &mut state.stage {
        stage.at = old;
    }
    state.turn_at = old;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_yes_after_the_question_commits_and_a_model_flag_does_not() {
        let _guard = hold_for_test();
        stage("place");
        assert!(!try_consume("place"), "the model cannot confirm in the same breath");
        note_user_turn("yesterday the weather was fine");
        assert!(!try_consume("place"));
        note_user_turn("no");
        assert!(!try_consume("place"));
        note_user_turn("yes");
        assert!(try_consume("place"));
        assert!(!try_consume("place"), "one yes confirms one action");
    }

    #[test]
    fn each_app_language_can_say_yes() {
        let _guard = hold_for_test();
        for word in ["yes", "yeah", "ok", "sí", "si", "vale", "ja", "klar", "sim", "val", "d'acord", "de acuerdo"] {
            reset_for_test();
            stage("reply");
            note_user_turn(word);
            assert!(try_consume("reply"), "{word}");
        }
        for word in ["no", "nein", "não", "nao", "cancela", "wait", "call mom"] {
            reset_for_test();
            stage("reply");
            note_user_turn(word);
            assert!(!try_consume("reply"), "{word}");
        }
    }

    #[test]
    fn a_tap_confirms_and_thirty_seconds_drops_it() {
        let _guard = hold_for_test();
        stage("answer");
        note_tap();
        assert!(try_consume("answer"));
        stage("end");
        note_user_turn("yes");
        expire_for_test();
        assert!(!try_consume("end"));
    }
}
