// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Server console command framework (plan 22 §3.6).
//!
//! Port of the `ServerControl.CommandHandler` parameter grammar
//! (`<required>`, `[optional]`, `...`), response kinds, the
//! `Strings.levenshtein` "did you mean" suggestion and the `yes` replay. Command
//! execution itself lives in [`super::commands`]; this module is pure parsing
//! plus the small `yes` state machine.

/// Parameter-signature failure (`CommandHandler` response kinds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgError {
    /// Too few arguments (`fewArguments`).
    FewArguments,
    /// Too many arguments (`manyArguments`).
    ManyArguments,
}

impl std::fmt::Display for ArgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArgError::FewArguments => write!(f, "Not enough arguments."),
            ArgError::ManyArguments => write!(f, "Too many arguments."),
        }
    }
}

/// Counts derived from a command's parameter text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamSpec {
    /// Minimum (`<required>`) argument count.
    pub required: usize,
    /// Optional (`[optional]`) argument count (bounded).
    pub optional: usize,
    /// Whether the last parameter is variadic (accepts any extra count).
    pub variadic: bool,
}

impl ParamSpec {
    /// Minimum accepted argument count.
    pub fn min(&self) -> usize {
        self.required
    }

    /// Maximum accepted argument count (`None` when variadic).
    pub fn max(&self) -> Option<usize> {
        if self.variadic {
            None
        } else {
            Some(self.required + self.optional)
        }
    }
}

/// Parses a `CommandHandler` parameter string, e.g. `"<name> [value...]"`.
pub fn parse_param_text(text: &str) -> ParamSpec {
    let mut required = 0;
    let mut optional = 0;
    let mut variadic = false;
    for token in text.split_whitespace() {
        let is_variadic = token.contains("...");
        let is_required = token.starts_with('<');
        if is_required {
            required += 1;
        } else if !is_variadic {
            // A variadic optional parameter accepts 0+, so it is not a bounded
            // optional slot.
            optional += 1;
        }
        if is_variadic {
            variadic = true;
        }
    }
    ParamSpec {
        required,
        optional,
        variadic,
    }
}

/// Validates an argument count against a signature.
pub fn validate(spec: ParamSpec, count: usize) -> Result<(), ArgError> {
    if count < spec.min() {
        return Err(ArgError::FewArguments);
    }
    if let Some(max) = spec.max()
        && count > max
    {
        return Err(ArgError::ManyArguments);
    }
    Ok(())
}

/// `Strings.levenshteinDistance` (two-row dynamic program).
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

/// The closest candidate within `Strings.levenshtein`'s `< 3` threshold.
pub fn suggest<'a>(word: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for candidate in candidates {
        let distance = levenshtein(word, candidate);
        if distance < 3
            && best
                .as_ref()
                .is_none_or(|(best_distance, _)| distance < *best_distance)
        {
            best = Some((distance, candidate.to_owned()));
        }
    }
    best.map(|(_, name)| name)
}

/// State for the `yes` command (`CommandHandler.lastCommand`-style replay).
#[derive(Debug, Default)]
pub struct ConsoleState {
    last_unknown: Option<String>,
    last_suggestion: Option<String>,
}

impl ConsoleState {
    /// Records an unknown command and its suggestion.
    pub fn record_unknown(&mut self, input: &str, suggestion: Option<String>) {
        self.last_unknown = Some(input.to_owned());
        self.last_suggestion = suggestion;
    }

    /// The command the `yes` command should replay, if any.
    pub fn resolve_yes(&self) -> Option<String> {
        self.last_suggestion.clone()
    }

    /// The last unrecognized input, if any.
    pub fn last_unknown(&self) -> Option<&str> {
        self.last_unknown.as_deref()
    }

    /// Clears the replay state after a successful `yes`.
    pub fn clear(&mut self) {
        self.last_unknown = None;
        self.last_suggestion = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_required_optional_variadic() {
        assert_eq!(
            parse_param_text("<name> [value...]"),
            ParamSpec {
                required: 1,
                optional: 0,
                variadic: true
            }
        );
        assert_eq!(
            parse_param_text("<a> <b> [c]"),
            ParamSpec {
                required: 2,
                optional: 1,
                variadic: false
            }
        );
        assert_eq!(
            parse_param_text(""),
            ParamSpec {
                required: 0,
                optional: 0,
                variadic: false
            }
        );
    }

    #[test]
    fn few_many_arguments() {
        let spec = parse_param_text("<a> [b] [c]");
        assert_eq!(validate(spec, 0), Err(ArgError::FewArguments));
        assert_eq!(validate(spec, 1), Ok(()));
        assert_eq!(validate(spec, 3), Ok(()));
        assert_eq!(validate(spec, 4), Err(ArgError::ManyArguments));

        let variadic = parse_param_text("<msg...>");
        assert_eq!(validate(variadic, 0), Err(ArgError::FewArguments));
        assert_eq!(validate(variadic, 100), Ok(()));
    }

    #[test]
    fn unknown_suggests_levenshtein() {
        assert_eq!(
            suggest("stauts", ["status", "help", "exit"]),
            Some("status".to_owned())
        );
        assert_eq!(suggest("zzzzz", ["status", "help", "exit"]), None);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
    }

    #[test]
    fn yes_replays_suggestion() {
        let mut state = ConsoleState::default();
        assert_eq!(state.resolve_yes(), None);
        state.record_unknown("hlep", Some("help".to_owned()));
        assert_eq!(state.last_unknown(), Some("hlep"));
        assert_eq!(state.resolve_yes(), Some("help".to_owned()));
        state.clear();
        assert_eq!(state.resolve_yes(), None);
    }
}
