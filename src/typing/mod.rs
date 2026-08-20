//! Portable compilation of AutoQuill documents into safe instructions.

mod instruction;
mod templating;
mod tokenizer;

use std::{error::Error, fmt};

pub use instruction::{Instruction, SpecialKey};
pub use templating::{
    RuntimeValueError, RuntimeValueProvider, RuntimeVariable, SystemRuntimeValues,
};
pub use tokenizer::MAX_KEY_REPETITIONS;

use templating::expand_runtime_variables;
use tokenizer::tokenize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningKind {
    UnknownRuntimeVariable,
    UnknownSpecialKey,
    InvalidRepetition,
    RepetitionLimitExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileWarning {
    pub kind: WarningKind,
    pub token: String,
    pub message: &'static str,
}

impl CompileWarning {
    #[must_use]
    pub fn new(kind: WarningKind, token: impl Into<String>, message: &'static str) -> Self {
        Self {
            kind,
            token: token.into(),
            message,
        }
    }
}

/// Immutable output used by simulation and, later, native input backends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compilation {
    pub expanded_text: String,
    pub instructions: Vec<Instruction>,
    pub warnings: Vec<CompileWarning>,
}

impl Compilation {
    #[must_use]
    pub fn instruction_count(&self) -> usize {
        self.instructions.len()
    }

    /// Count only intended character actions; special keys do not inflate text progress.
    #[must_use]
    pub fn character_instruction_count(&self) -> usize {
        self.instructions
            .iter()
            .filter(|instruction| matches!(instruction, Instruction::Character(_)))
            .count()
    }
}

#[derive(Debug)]
pub enum CompileError {
    RuntimeValue {
        variable: RuntimeVariable,
        source: RuntimeValueError,
    },
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RuntimeValue { variable, source } => {
                write!(
                    formatter,
                    "Could not resolve {{{}}}: {source}",
                    variable.name()
                )
            }
        }
    }
}

impl Error for CompileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::RuntimeValue { source, .. } => Some(source),
        }
    }
}

/// Expand current values, then parse the complete document into portable instructions.
pub fn compile(
    text: &str,
    provider: &impl RuntimeValueProvider,
) -> Result<Compilation, CompileError> {
    let expansion =
        expand_runtime_variables(text, provider).map_err(|error| CompileError::RuntimeValue {
            variable: error.variable,
            source: error.source,
        })?;
    let tokenization = tokenize(&expansion.text);
    let mut warnings = expansion.warnings;
    warnings.extend(tokenization.warnings);

    Ok(Compilation {
        expanded_text: expansion.text,
        instructions: tokenization.instructions,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeValues;

    impl RuntimeValueProvider for FakeValues {
        fn resolve(&self, variable: RuntimeVariable) -> Result<String, RuntimeValueError> {
            Ok(match variable {
                RuntimeVariable::Clipboard => "Hello",
                RuntimeVariable::Date => "2026-08-21",
                RuntimeVariable::Time => "18:00:00",
            }
            .into())
        }
    }

    #[test]
    fn compilation_expands_before_tokenizing() {
        let compiled = compile("{CLIPBOARD}[ENTER]{DATE}", &FakeValues).unwrap();
        assert_eq!(compiled.expanded_text, "Hello[ENTER]2026-08-21");
        assert_eq!(compiled.instruction_count(), 16);
        assert_eq!(compiled.character_instruction_count(), 15);
        assert_eq!(
            compiled.instructions[5],
            Instruction::SpecialKey(SpecialKey::Enter)
        );
    }
}
