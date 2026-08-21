//! Portable non-blocking session engine for safe simulation and future input backends.

use std::{collections::VecDeque, error::Error, fmt, time::Duration};

use crate::domain::{SessionState, TypingSettings};

use super::{
    BreakScheduler, ErrorScheduler, Instruction, RandomSource, SeededRandom, ShortPauseScheduler,
    SpecialKey, TimingModel,
};

const TYPO_CHARACTERS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const MAX_OPERATIONS_PER_ADVANCE: usize = 4_096;

#[derive(Debug, Clone)]
pub struct SessionPlan {
    pub instructions: Vec<Instruction>,
    pub settings: TypingSettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewOperation {
    Intended(Instruction),
    TypoCharacter(char),
    CorrectionBackspace,
}

impl PreviewOperation {
    #[must_use]
    pub fn action_label(&self) -> String {
        match self {
            Self::Intended(instruction) => instruction.action_label(),
            Self::TypoCharacter(character) => format!("Simulated typo · {character}"),
            Self::CorrectionBackspace => "Correction · Backspace".into(),
        }
    }

    #[must_use]
    pub fn scheduler_instruction(&self) -> Instruction {
        match self {
            Self::Intended(instruction) => instruction.clone(),
            Self::TypoCharacter(character) => Instruction::Character(*character),
            Self::CorrectionBackspace => Instruction::SpecialKey(SpecialKey::Backspace),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionReason {
    Finished,
    StopAfterReached,
    UserStopped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineSnapshot {
    pub session_id: u64,
    pub state: SessionState,
    pub progress: f32,
    pub completed_instructions: usize,
    pub total_instructions: usize,
    pub typed_characters: usize,
    pub total_characters: usize,
    pub pass: u32,
    pub active_elapsed: Duration,
    pub remaining_wait: Duration,
    pub current_action: String,
    pub completion_reason: Option<CompletionReason>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineUpdate {
    pub snapshot: EngineSnapshot,
    pub operations: Vec<PreviewOperation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionAlreadyActive;

impl fmt::Display for SessionAlreadyActive {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("A simulation session is already active.")
    }
}

impl Error for SessionAlreadyActive {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum WaitKind {
    #[default]
    None,
    Startup,
    BeforeOperation,
    ShortPause,
    Break,
    Loop,
}

#[derive(Debug)]
pub struct SessionEngine<R: RandomSource> {
    random: R,
    session_id: u64,
    state: SessionState,
    resume_state: SessionState,
    plan: Option<SessionPlan>,
    timing: Option<TimingModel>,
    break_scheduler: Option<BreakScheduler>,
    pause_scheduler: Option<ShortPauseScheduler>,
    error_scheduler: Option<ErrorScheduler>,
    pending_operations: VecDeque<PreviewOperation>,
    pending_waits: VecDeque<(WaitKind, Duration)>,
    wait_kind: WaitKind,
    remaining_wait: Duration,
    cursor: usize,
    typed_characters: usize,
    total_characters: usize,
    pass: u32,
    active_elapsed: Duration,
    current_action: String,
    completion_reason: Option<CompletionReason>,
}

impl SessionEngine<SeededRandom> {
    #[must_use]
    pub fn from_system_time() -> Self {
        Self::new(SeededRandom::from_system_time())
    }
}

impl<R: RandomSource> SessionEngine<R> {
    #[must_use]
    pub fn new(random: R) -> Self {
        Self {
            random,
            session_id: 0,
            state: SessionState::Idle,
            resume_state: SessionState::Idle,
            plan: None,
            timing: None,
            break_scheduler: None,
            pause_scheduler: None,
            error_scheduler: None,
            pending_operations: VecDeque::new(),
            pending_waits: VecDeque::new(),
            wait_kind: WaitKind::None,
            remaining_wait: Duration::ZERO,
            cursor: 0,
            typed_characters: 0,
            total_characters: 0,
            pass: 0,
            active_elapsed: Duration::ZERO,
            current_action: "Waiting for text".into(),
            completion_reason: None,
        }
    }

    pub fn start(&mut self, plan: SessionPlan) -> Result<EngineUpdate, SessionAlreadyActive> {
        if self.state.is_active() {
            return Err(SessionAlreadyActive);
        }

        self.session_id = self.session_id.wrapping_add(1).max(1);
        self.cursor = 0;
        self.typed_characters = 0;
        self.total_characters = plan
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction, Instruction::Character(_)))
            .count();
        self.pass = 1;
        self.active_elapsed = Duration::ZERO;
        self.pending_operations.clear();
        self.pending_waits.clear();
        self.completion_reason = None;
        self.timing = Some(TimingModel::new(
            plan.settings.wpm,
            plan.settings.breaks,
            plan.settings.pauses,
        ));
        self.break_scheduler = Some(BreakScheduler::new(plan.settings.breaks, &mut self.random));
        self.pause_scheduler = Some(ShortPauseScheduler::new(
            plan.settings.pauses,
            &mut self.random,
        ));
        self.error_scheduler = Some(ErrorScheduler::new(plan.settings.errors, &mut self.random));
        self.plan = Some(plan);

        if self.total_instructions() == 0 {
            self.complete(CompletionReason::Finished, "Nothing to simulate");
        } else if self.settings().startup_delay_enabled {
            self.state = SessionState::Countdown;
            self.set_wait(
                WaitKind::Startup,
                Duration::from_secs(u64::from(self.settings().startup_delay_seconds)),
                "Startup countdown",
            );
        } else {
            self.state = SessionState::Typing;
            self.current_action = "Preparing first action".into();
            self.schedule_next();
        }

        Ok(self.update(Vec::new()))
    }

    pub fn pause(&mut self) -> EngineUpdate {
        if matches!(
            self.state,
            SessionState::Countdown | SessionState::Typing | SessionState::LoopWait
        ) {
            self.resume_state = self.state;
            self.state = SessionState::Paused;
            self.current_action = "Paused — active-time limit is frozen".into();
        }
        self.update(Vec::new())
    }

    pub fn resume(&mut self) -> EngineUpdate {
        if self.state == SessionState::Paused {
            self.state = self.resume_state;
            self.current_action = match self.wait_kind {
                WaitKind::Startup => "Startup countdown",
                WaitKind::ShortPause => "Short pause",
                WaitKind::Break => "Human-like break",
                WaitKind::Loop => "Waiting before next pass",
                _ => "Resuming simulation",
            }
            .into();
        }
        self.update(Vec::new())
    }

    pub fn stop(&mut self) -> EngineUpdate {
        if self.state.is_active() {
            self.state = SessionState::Idle;
            self.wait_kind = WaitKind::None;
            self.remaining_wait = Duration::ZERO;
            self.pending_operations.clear();
            self.pending_waits.clear();
            self.completion_reason = Some(CompletionReason::UserStopped);
            self.current_action = "Stopped safely".into();
        }
        self.update(Vec::new())
    }

    /// End an active session after a native backend safety check or input operation fails.
    pub fn fail(&mut self, action: impl Into<String>) -> EngineUpdate {
        if self.state.is_active() {
            self.state = SessionState::Failed;
            self.wait_kind = WaitKind::None;
            self.remaining_wait = Duration::ZERO;
            self.pending_operations.clear();
            self.pending_waits.clear();
            self.completion_reason = None;
            self.current_action = action.into();
        }
        self.update(Vec::new())
    }

    pub fn reset(&mut self) -> EngineUpdate {
        self.state = SessionState::Idle;
        self.resume_state = SessionState::Idle;
        self.plan = None;
        self.timing = None;
        self.break_scheduler = None;
        self.pause_scheduler = None;
        self.error_scheduler = None;
        self.pending_operations.clear();
        self.pending_waits.clear();
        self.wait_kind = WaitKind::None;
        self.remaining_wait = Duration::ZERO;
        self.cursor = 0;
        self.typed_characters = 0;
        self.total_characters = 0;
        self.pass = 0;
        self.active_elapsed = Duration::ZERO;
        self.current_action = "Waiting for text".into();
        self.completion_reason = None;
        self.update(Vec::new())
    }

    pub fn advance(&mut self, elapsed: Duration) -> EngineUpdate {
        if !matches!(
            self.state,
            SessionState::Countdown | SessionState::Typing | SessionState::LoopWait
        ) {
            return self.update(Vec::new());
        }

        let mut budget = elapsed;
        let mut operations = Vec::new();
        let mut iterations = 0;

        loop {
            if self.stop_after_reached() {
                self.complete(
                    CompletionReason::StopAfterReached,
                    "Stopped at the configured active-time limit",
                );
                break;
            }
            if !matches!(
                self.state,
                SessionState::Countdown | SessionState::Typing | SessionState::LoopWait
            ) {
                break;
            }
            if operations.len() >= MAX_OPERATIONS_PER_ADVANCE {
                self.current_action = "Catching up safely".into();
                break;
            }

            if self.remaining_wait > budget {
                self.remaining_wait -= budget;
                self.active_elapsed += budget;
                break;
            }

            let consumed = self.remaining_wait;
            self.active_elapsed += consumed;
            budget = budget.saturating_sub(consumed);
            self.remaining_wait = Duration::ZERO;
            self.finish_wait(&mut operations);
            iterations += 1;

            if iterations > MAX_OPERATIONS_PER_ADVANCE * 4 {
                self.current_action = "Catching up safely".into();
                break;
            }
            if budget.is_zero() && !self.remaining_wait.is_zero() {
                break;
            }
        }

        self.update(operations)
    }

    #[must_use]
    pub fn snapshot(&self) -> EngineSnapshot {
        let total_instructions = self.total_instructions();
        let progress = if self.total_characters > 0 {
            self.typed_characters as f32 / self.total_characters as f32
        } else if self.state == SessionState::Completed {
            1.0
        } else {
            0.0
        };
        EngineSnapshot {
            session_id: self.session_id,
            state: self.state,
            progress: progress.clamp(0.0, 1.0),
            completed_instructions: self.cursor,
            total_instructions,
            typed_characters: self.typed_characters,
            total_characters: self.total_characters,
            pass: self.pass,
            active_elapsed: self.active_elapsed,
            remaining_wait: self.remaining_wait,
            current_action: self.current_action.clone(),
            completion_reason: self.completion_reason,
        }
    }

    fn finish_wait(&mut self, operations: &mut Vec<PreviewOperation>) {
        let completed_wait = std::mem::take(&mut self.wait_kind);
        match completed_wait {
            WaitKind::Startup => {
                self.state = SessionState::Typing;
                self.current_action = "Countdown complete".into();
                self.schedule_next();
            }
            WaitKind::BeforeOperation => self.emit_next_operation(operations),
            WaitKind::ShortPause | WaitKind::Break => self.schedule_next(),
            WaitKind::Loop => self.begin_next_pass(),
            WaitKind::None => self.schedule_next(),
        }
    }

    fn emit_next_operation(&mut self, operations: &mut Vec<PreviewOperation>) {
        let (operation, intended) = if let Some(operation) = self.pending_operations.pop_front() {
            (operation, false)
        } else if let Some(instruction) = self
            .plan
            .as_ref()
            .and_then(|plan| plan.instructions.get(self.cursor))
            .cloned()
        {
            self.cursor += 1;
            (PreviewOperation::Intended(instruction), true)
        } else {
            self.finish_pass();
            return;
        };

        self.current_action = operation.action_label();
        let scheduler_instruction = operation.scheduler_instruction();
        operations.push(operation.clone());

        if intended
            && matches!(
                operation,
                PreviewOperation::Intended(Instruction::Character(_))
            )
        {
            self.typed_characters += 1;
            if let Some(pause) = self
                .pause_scheduler
                .as_mut()
                .and_then(|scheduler| scheduler.step_intended_character(&mut self.random))
            {
                self.pending_waits.push_back((WaitKind::ShortPause, pause));
            }
        }

        if let Some(break_duration) = self
            .break_scheduler
            .as_mut()
            .and_then(|scheduler| scheduler.step(&scheduler_instruction, &mut self.random))
        {
            self.pending_waits
                .push_back((WaitKind::Break, break_duration));
        }

        if intended
            && let Some(error_count) = self
                .error_scheduler
                .as_mut()
                .and_then(|scheduler| scheduler.step_intended_token(&mut self.random))
        {
            self.queue_simulated_errors(error_count);
        }

        self.schedule_next();
    }

    fn queue_simulated_errors(&mut self, count: u32) {
        for _ in 0..count {
            let index = self
                .random
                .integer_inclusive(0, (TYPO_CHARACTERS.len() - 1) as u32)
                as usize;
            self.pending_operations
                .push_back(PreviewOperation::TypoCharacter(
                    TYPO_CHARACTERS[index] as char,
                ));
        }
        self.pending_operations.extend(std::iter::repeat_n(
            PreviewOperation::CorrectionBackspace,
            count as usize,
        ));
    }

    fn schedule_next(&mut self) {
        if let Some((kind, duration)) = self.pending_waits.pop_front() {
            let label = match kind {
                WaitKind::ShortPause => format_duration("Short pause", duration),
                WaitKind::Break => format_duration("Human-like break", duration),
                _ => "Waiting".into(),
            };
            self.set_wait(kind, duration, &label);
            return;
        }

        if !self.pending_operations.is_empty() || self.cursor < self.total_instructions() {
            self.state = SessionState::Typing;
            let delay = self
                .timing
                .expect("active sessions have a timing model")
                .next_operation_delay(&mut self.random);
            self.set_wait(WaitKind::BeforeOperation, delay, "");
        } else {
            self.finish_pass();
        }
    }

    fn finish_pass(&mut self) {
        let looping = self.settings().looping;
        if looping.enabled {
            if let Some(scheduler) = self.break_scheduler.as_mut() {
                scheduler.reset_after_loop(&mut self.random);
            }
            let seconds = self
                .random
                .integer_inclusive(looping.min_seconds, looping.max_seconds);
            self.state = SessionState::LoopWait;
            let duration = Duration::from_secs(u64::from(seconds));
            self.set_wait(
                WaitKind::Loop,
                duration,
                &format_duration("Loop wait", duration),
            );
        } else {
            self.complete(CompletionReason::Finished, "Simulation complete");
        }
    }

    fn begin_next_pass(&mut self) {
        self.pass = self.pass.saturating_add(1);
        self.cursor = 0;
        self.typed_characters = 0;
        if let Some(scheduler) = self.pause_scheduler.as_mut() {
            scheduler.reset(&mut self.random);
        }
        if let Some(scheduler) = self.error_scheduler.as_mut() {
            scheduler.reset(&mut self.random);
        }
        self.state = SessionState::Typing;
        self.current_action = format!("Starting pass {}", self.pass);
        self.schedule_next();
    }

    fn complete(&mut self, reason: CompletionReason, action: &str) {
        self.state = SessionState::Completed;
        self.wait_kind = WaitKind::None;
        self.remaining_wait = Duration::ZERO;
        self.pending_operations.clear();
        self.pending_waits.clear();
        self.completion_reason = Some(reason);
        self.current_action = action.into();
    }

    fn set_wait(&mut self, kind: WaitKind, duration: Duration, action: &str) {
        self.wait_kind = kind;
        self.remaining_wait = duration;
        if !action.is_empty() {
            self.current_action = action.into();
        }
    }

    fn stop_after_reached(&self) -> bool {
        self.plan.as_ref().is_some_and(|plan| {
            plan.settings.stop_after.enabled
                && self.active_elapsed
                    >= Duration::from_secs(u64::from(plan.settings.stop_after.seconds))
        })
    }

    fn settings(&self) -> &TypingSettings {
        &self
            .plan
            .as_ref()
            .expect("active sessions have a plan")
            .settings
    }

    fn total_instructions(&self) -> usize {
        self.plan.as_ref().map_or(0, |plan| plan.instructions.len())
    }

    fn update(&self, operations: Vec<PreviewOperation>) -> EngineUpdate {
        EngineUpdate {
            snapshot: self.snapshot(),
            operations,
        }
    }
}

fn format_duration(label: &str, duration: Duration) -> String {
    if duration.as_secs() > 0 {
        format!("{label} · {:.1} s", duration.as_secs_f64())
    } else {
        format!("{label} · {} ms", duration.as_millis())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ErrorSettings, LoopSettings, StopAfterSettings, Wpm};

    fn plan(text: &str, settings: TypingSettings) -> SessionPlan {
        SessionPlan {
            instructions: text.chars().map(Instruction::Character).collect(),
            settings,
        }
    }

    fn fast_settings() -> TypingSettings {
        TypingSettings {
            wpm: Wpm::new(200),
            ..TypingSettings::default()
        }
    }

    fn drain(engine: &mut SessionEngine<SeededRandom>) -> Vec<PreviewOperation> {
        let mut operations = Vec::new();
        for _ in 0..100 {
            let update = engine.advance(Duration::from_secs(1));
            operations.extend(update.operations);
            if update.snapshot.state == SessionState::Completed {
                return operations;
            }
        }
        panic!("session did not complete");
    }

    #[test]
    fn deterministic_session_completes_in_order() {
        let mut engine = SessionEngine::new(SeededRandom::new(11));
        engine.start(plan("abc", fast_settings())).unwrap();
        let operations = drain(&mut engine);
        assert_eq!(
            operations,
            vec![
                PreviewOperation::Intended(Instruction::Character('a')),
                PreviewOperation::Intended(Instruction::Character('b')),
                PreviewOperation::Intended(Instruction::Character('c')),
            ]
        );
        let snapshot = engine.snapshot();
        assert_eq!(snapshot.state, SessionState::Completed);
        assert_eq!(snapshot.progress, 1.0);
        assert_eq!(snapshot.completion_reason, Some(CompletionReason::Finished));
    }

    #[test]
    fn paused_time_does_not_count_toward_stop_after() {
        let settings = TypingSettings {
            stop_after: StopAfterSettings {
                enabled: true,
                seconds: 1,
            },
            ..fast_settings()
        };
        let mut engine = SessionEngine::new(SeededRandom::new(22));
        engine.start(plan(&"x".repeat(100), settings)).unwrap();
        engine.advance(Duration::from_millis(400));
        let active_before_pause = engine.snapshot().active_elapsed;
        engine.pause();
        engine.advance(Duration::from_secs(30));
        assert_eq!(engine.snapshot().active_elapsed, active_before_pause);
        assert_eq!(engine.snapshot().state, SessionState::Paused);
        engine.resume();
        let update = engine.advance(Duration::from_secs(1));
        assert_eq!(update.snapshot.state, SessionState::Completed);
        assert_eq!(
            update.snapshot.completion_reason,
            Some(CompletionReason::StopAfterReached)
        );
    }

    #[test]
    fn stop_prevents_every_later_operation() {
        let mut engine = SessionEngine::new(SeededRandom::new(33));
        engine.start(plan("abcdef", fast_settings())).unwrap();
        assert!(
            !engine
                .advance(Duration::from_millis(100))
                .operations
                .is_empty()
        );
        engine.stop();
        let after_stop = engine.advance(Duration::from_secs(100));
        assert!(after_stop.operations.is_empty());
        assert_eq!(after_stop.snapshot.state, SessionState::Idle);
    }

    #[test]
    fn backend_failure_cancels_pending_input_permanently() {
        let mut engine = SessionEngine::new(SeededRandom::new(34));
        engine.start(plan("abcdef", fast_settings())).unwrap();
        let failed = engine.fail("Target focus changed");
        assert_eq!(failed.snapshot.state, SessionState::Failed);
        assert_eq!(failed.snapshot.current_action, "Target focus changed");
        assert!(failed.operations.is_empty());

        let after_failure = engine.advance(Duration::from_secs(100));
        assert_eq!(after_failure.snapshot.state, SessionState::Failed);
        assert!(after_failure.operations.is_empty());
    }

    #[test]
    fn simulated_errors_do_not_inflate_progress() {
        let settings = TypingSettings {
            errors: ErrorSettings {
                enabled: true,
                min_interval: 1,
                max_interval: 1,
                min_errors: 2,
                max_errors: 2,
            },
            ..fast_settings()
        };
        let mut engine = SessionEngine::new(SeededRandom::new(44));
        engine.start(plan("a", settings)).unwrap();
        let operations = drain(&mut engine);
        assert_eq!(operations.len(), 5);
        assert!(matches!(operations[0], PreviewOperation::Intended(_)));
        assert!(matches!(operations[1], PreviewOperation::TypoCharacter(_)));
        assert!(matches!(operations[2], PreviewOperation::TypoCharacter(_)));
        assert_eq!(operations[3], PreviewOperation::CorrectionBackspace);
        assert_eq!(operations[4], PreviewOperation::CorrectionBackspace);
        assert_eq!(engine.snapshot().typed_characters, 1);
        assert_eq!(engine.snapshot().progress, 1.0);
    }

    #[test]
    fn loop_wait_resets_per_pass_progress() {
        let settings = TypingSettings {
            looping: LoopSettings {
                enabled: true,
                min_seconds: 0,
                max_seconds: 0,
            },
            ..fast_settings()
        };
        let mut engine = SessionEngine::new(SeededRandom::new(55));
        engine.start(plan("ab", settings)).unwrap();
        engine.advance(Duration::from_millis(500));
        assert!(engine.snapshot().pass >= 2);
        assert!(engine.snapshot().state.is_active());
        engine.stop();
    }
}
