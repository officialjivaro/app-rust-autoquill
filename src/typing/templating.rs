//! Runtime placeholder expansion performed immediately before a session starts.

use std::{error::Error, fmt};

use copypasta::{ClipboardContext, ClipboardProvider};

use super::{CompileWarning, WarningKind};

/// Runtime value names supported by the original AutoQuill application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeVariable {
    Clipboard,
    Date,
    Time,
}

impl RuntimeVariable {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Clipboard => "CLIPBOARD",
            Self::Date => "DATE",
            Self::Time => "TIME",
        }
    }

    #[must_use]
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "CLIPBOARD" => Some(Self::Clipboard),
            "DATE" => Some(Self::Date),
            "TIME" => Some(Self::Time),
            _ => None,
        }
    }
}

/// Source of values that change between simulation runs.
pub trait RuntimeValueProvider {
    fn resolve(&self, variable: RuntimeVariable) -> Result<String, RuntimeValueError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeValueError {
    message: String,
}

impl RuntimeValueError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for RuntimeValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for RuntimeValueError {}

/// Production runtime values. Clipboard access only occurs when requested by the compiler.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemRuntimeValues;

impl RuntimeValueProvider for SystemRuntimeValues {
    fn resolve(&self, variable: RuntimeVariable) -> Result<String, RuntimeValueError> {
        match variable {
            RuntimeVariable::Clipboard => ClipboardContext::new()
                .and_then(|mut clipboard| clipboard.get_contents())
                .map_err(|error| {
                    RuntimeValueError::new(format!("Clipboard text is unavailable: {error}"))
                }),
            RuntimeVariable::Date => Ok(local_date_and_time()?.0),
            RuntimeVariable::Time => Ok(local_date_and_time()?.1),
        }
    }
}

pub(crate) struct Expansion {
    pub text: String,
    pub warnings: Vec<CompileWarning>,
}

pub(crate) fn expand_runtime_variables(
    text: &str,
    provider: &impl RuntimeValueProvider,
) -> Result<Expansion, RuntimeExpansionError> {
    let mut output = String::with_capacity(text.len());
    let mut warnings = Vec::new();
    let mut index = 0;

    while index < text.len() {
        let remaining = &text[index..];

        if let Some(escaped) = remaining.strip_prefix("\"\"{")
            && let Some(end) = escaped.find("}\"\"")
        {
            output.push('{');
            output.push_str(&escaped[..end]);
            output.push('}');
            index += 3 + end + 3;
            continue;
        }

        if remaining.starts_with('{')
            && let Some(end) = remaining.find('}')
        {
            let name = &remaining[1..end];
            if !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_uppercase()) {
                let token = &remaining[..=end];
                if let Some(variable) = RuntimeVariable::from_name(name) {
                    let value = provider
                        .resolve(variable)
                        .map_err(|source| RuntimeExpansionError { variable, source })?;
                    output.push_str(&value);
                } else {
                    output.push_str(token);
                    warnings.push(CompileWarning::new(
                        WarningKind::UnknownRuntimeVariable,
                        token,
                        "Unknown runtime variable kept as literal text.",
                    ));
                }
                index += end + 1;
                continue;
            }
        }

        let character = remaining
            .chars()
            .next()
            .expect("index is on a character boundary");
        output.push(character);
        index += character.len_utf8();
    }

    Ok(Expansion {
        text: output,
        warnings,
    })
}

#[derive(Debug)]
pub(crate) struct RuntimeExpansionError {
    pub variable: RuntimeVariable,
    pub source: RuntimeValueError,
}

#[cfg(windows)]
fn local_date_and_time() -> Result<(String, String), RuntimeValueError> {
    use windows_sys::Win32::{Foundation::SYSTEMTIME, System::SystemInformation::GetLocalTime};

    let mut value = SYSTEMTIME::default();
    // SAFETY: `GetLocalTime` writes to a valid, initialized SYSTEMTIME pointer.
    unsafe { GetLocalTime(&mut value) };
    Ok((
        format!("{:04}-{:02}-{:02}", value.wYear, value.wMonth, value.wDay),
        format!(
            "{:02}:{:02}:{:02}",
            value.wHour, value.wMinute, value.wSecond
        ),
    ))
}

#[cfg(unix)]
fn local_date_and_time() -> Result<(String, String), RuntimeValueError> {
    let mut timestamp: libc::time_t = 0;
    // SAFETY: both pointers refer to valid writable storage for their documented C types.
    let local = unsafe {
        if libc::time(&mut timestamp).is_negative() {
            return Err(RuntimeValueError::new("The system clock is unavailable."));
        }
        let mut local: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&timestamp, &mut local).is_null() {
            return Err(RuntimeValueError::new(
                "The local time zone is unavailable.",
            ));
        }
        local
    };
    Ok((
        format!(
            "{:04}-{:02}-{:02}",
            local.tm_year + 1900,
            local.tm_mon + 1,
            local.tm_mday
        ),
        format!(
            "{:02}:{:02}:{:02}",
            local.tm_hour, local.tm_min, local.tm_sec
        ),
    ))
}

#[cfg(not(any(windows, unix)))]
fn local_date_and_time() -> Result<(String, String), RuntimeValueError> {
    Err(RuntimeValueError::new(
        "Local date and time are not implemented on this platform.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeValues;

    impl RuntimeValueProvider for FakeValues {
        fn resolve(&self, variable: RuntimeVariable) -> Result<String, RuntimeValueError> {
            Ok(match variable {
                RuntimeVariable::Clipboard => "copied text",
                RuntimeVariable::Date => "2026-08-21",
                RuntimeVariable::Time => "14:35:09",
            }
            .into())
        }
    }

    #[test]
    fn supported_values_expand_and_escaped_values_stay_literal() {
        let expanded =
            expand_runtime_variables("{DATE} {TIME} {CLIPBOARD} \"\"{DATE}\"\"", &FakeValues)
                .expect("fake values cannot fail");
        assert_eq!(expanded.text, "2026-08-21 14:35:09 copied text {DATE}");
        assert!(expanded.warnings.is_empty());
    }

    #[test]
    fn unknown_runtime_values_are_preserved_with_a_warning() {
        let expanded = expand_runtime_variables("Keep {PROJECT}.", &FakeValues)
            .expect("fake values cannot fail");
        assert_eq!(expanded.text, "Keep {PROJECT}.");
        assert_eq!(expanded.warnings.len(), 1);
    }
}
