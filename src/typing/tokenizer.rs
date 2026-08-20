//! Manual parser for AutoQuill's `[SPECIAL_KEY]` syntax.

use super::{CompileWarning, Instruction, SpecialKey, WarningKind};

/// Prevent an accidentally pasted multiplier from allocating an unbounded instruction list.
pub const MAX_KEY_REPETITIONS: usize = 10_000;

pub(crate) struct Tokenization {
    pub instructions: Vec<Instruction>,
    pub warnings: Vec<CompileWarning>,
}

pub(crate) fn tokenize(text: &str) -> Tokenization {
    let mut instructions = Vec::with_capacity(text.chars().count());
    let mut warnings = Vec::new();
    let mut index = 0;

    while index < text.len() {
        let remaining = &text[index..];

        if let Some(escaped) = remaining.strip_prefix("\"\"[")
            && let Some(end) = escaped.find("]\"\"")
        {
            push_characters(&mut instructions, &format!("[{}]", &escaped[..end]));
            index += 3 + end + 3;
            continue;
        }

        if remaining.starts_with("\r\n") {
            instructions.push(Instruction::SpecialKey(SpecialKey::Enter));
            index += 2;
            continue;
        }

        if remaining.starts_with(['\r', '\n']) {
            instructions.push(Instruction::SpecialKey(SpecialKey::Enter));
            index += 1;
            continue;
        }

        if remaining.starts_with('[')
            && let Some(end) = remaining.find(']')
        {
            let token = &remaining[..=end];
            let body = &remaining[1..end];
            if looks_like_key_token(body) {
                match parse_key_token(body) {
                    Ok((key, repetitions)) => {
                        instructions.extend(std::iter::repeat_n(
                            Instruction::SpecialKey(key),
                            repetitions,
                        ));
                        index += end + 1;
                        continue;
                    }
                    Err((kind, message)) => {
                        push_characters(&mut instructions, token);
                        warnings.push(CompileWarning::new(kind, token, message));
                        index += end + 1;
                        continue;
                    }
                }
            }
        }

        let character = remaining
            .chars()
            .next()
            .expect("index is on a character boundary");
        instructions.push(Instruction::Character(character));
        index += character.len_utf8();
    }

    Tokenization {
        instructions,
        warnings,
    }
}

fn looks_like_key_token(body: &str) -> bool {
    !body.is_empty()
        && body.as_bytes()[0].is_ascii_uppercase()
        && body
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'*')
}

fn parse_key_token(body: &str) -> Result<(SpecialKey, usize), (WarningKind, &'static str)> {
    let (name, repetitions) = match body.split_once('*') {
        Some((name, amount))
            if !name.is_empty()
                && !amount.is_empty()
                && !amount.contains('*')
                && amount.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            let repetitions = amount.parse::<usize>().map_err(|_| {
                (
                    WarningKind::InvalidRepetition,
                    "Special-key repetition is too large and was kept as literal text.",
                )
            })?;
            (name, repetitions)
        }
        Some(_) => {
            return Err((
                WarningKind::InvalidRepetition,
                "Malformed special-key repetition was kept as literal text.",
            ));
        }
        None => (body, 1),
    };

    if repetitions > MAX_KEY_REPETITIONS {
        return Err((
            WarningKind::RepetitionLimitExceeded,
            "Special-key repetition exceeds the safe limit and was kept as literal text.",
        ));
    }

    SpecialKey::from_name(name)
        .map(|key| (key, repetitions))
        .ok_or((
            WarningKind::UnknownSpecialKey,
            "Unknown special key kept as literal text.",
        ))
}

fn push_characters(instructions: &mut Vec<Instruction>, text: &str) {
    instructions.extend(text.chars().map(Instruction::Character));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newlines_and_repetitions_match_python_behavior() {
        let result = tokenize("a\r\nb\rc\n[TAB*3]");
        assert_eq!(
            result.instructions,
            vec![
                Instruction::Character('a'),
                Instruction::SpecialKey(SpecialKey::Enter),
                Instruction::Character('b'),
                Instruction::SpecialKey(SpecialKey::Enter),
                Instruction::Character('c'),
                Instruction::SpecialKey(SpecialKey::Enter),
                Instruction::SpecialKey(SpecialKey::Tab),
                Instruction::SpecialKey(SpecialKey::Tab),
                Instruction::SpecialKey(SpecialKey::Tab),
            ]
        );
    }

    #[test]
    fn escaped_and_unknown_keys_stay_literal() {
        let result = tokenize("\"\"[ENTER]\"\" [NOTAKEY] [TAB*x]");
        let preview: String = result
            .instructions
            .iter()
            .map(Instruction::preview_fragment)
            .collect();
        assert_eq!(preview, "[ENTER] [NOTAKEY] [TAB*x]");
        assert_eq!(result.warnings.len(), 2);
        assert_eq!(result.warnings[0].kind, WarningKind::UnknownSpecialKey);
        assert_eq!(result.warnings[1].kind, WarningKind::InvalidRepetition);
    }

    #[test]
    fn zero_repetitions_are_consumed_like_the_original_parser() {
        let result = tokenize("a[TAB*0]b");
        assert_eq!(
            result.instructions,
            vec![Instruction::Character('a'), Instruction::Character('b')]
        );
    }

    #[test]
    fn excessive_repetition_is_safe_and_literal() {
        let result = tokenize("[ENTER*10001]");
        assert_eq!(result.instructions.len(), 13);
        assert_eq!(
            result.warnings[0].kind,
            WarningKind::RepetitionLimitExceeded
        );
    }

    #[test]
    fn unicode_is_preserved_as_scalar_instructions() {
        let result = tokenize("日本語🙂");
        assert_eq!(result.instructions.len(), 4);
    }
}
