//! Process-local policy for the Infinity continuation CLI modes.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

use super::agent::GoalDisplayStatus;
use super::agent_view::AgentView;

static AUTO_NEXT_STEPS: AtomicBool = AtomicBool::new(false);
static AUTO_NEXT_IDEA: AtomicBool = AtomicBool::new(false);
static AUTO_NEXT_GOAL: AtomicBool = AtomicBool::new(false);
static CHAINED_GOALS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

pub(super) fn configure(steps: bool, idea: bool, goal: bool) {
    AUTO_NEXT_STEPS.store(steps, Ordering::Relaxed);
    AUTO_NEXT_IDEA.store(idea, Ordering::Relaxed);
    AUTO_NEXT_GOAL.store(goal, Ordering::Relaxed);
    CHAINED_GOALS.lock().expect("auto-next goal lock").clear();
}

pub(super) fn next_prompt(agent: &AgentView) -> Option<String> {
    if AUTO_NEXT_GOAL.load(Ordering::Relaxed)
        && let Some(goal) = agent.goal_state.as_ref()
        && goal.status == GoalDisplayStatus::Complete
    {
        let session_id = agent
            .session
            .session_id
            .as_ref()
            .map(|id| id.0.as_ref())
            .unwrap_or("new-session");
        let key = format!("{session_id}:{}", goal.goal_id);
        if CHAINED_GOALS
            .lock()
            .expect("auto-next goal lock")
            .insert(key)
        {
            return Some(format!(
                "/goal Review the completed goal {:?} and the current repository state. Choose the highest-value concrete next objective that builds on it, then implement and verify that objective end to end.",
                goal.objective
            ));
        }
    }

    match (
        AUTO_NEXT_STEPS.load(Ordering::Relaxed),
        AUTO_NEXT_IDEA.load(Ordering::Relaxed),
    ) {
        (true, true) => Some(
            "Continue autonomously. First complete the most important natural next steps, including relevant tests and validation. If the current work is genuinely complete, identify a fresh high-value improvement for this repository and implement it end to end. Do not stop merely to suggest work; perform it."
                .to_string(),
        ),
        (true, false) => Some(
            "Continue with the most important natural next steps. Implement them, run the relevant tests or validation, and resolve any failures you can. Do not stop merely to suggest the next steps; perform them."
                .to_string(),
        ),
        (false, true) => Some(
            "Inspect the current repository and recent work, identify a fresh high-value improvement, and implement it end to end with relevant validation. Prefer a concrete useful idea over a cosmetic change."
                .to_string(),
        ),
        (false, false) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_modes_do_not_continue() {
        configure(false, false, false);
        assert!(!AUTO_NEXT_STEPS.load(Ordering::Relaxed));
        assert!(!AUTO_NEXT_IDEA.load(Ordering::Relaxed));
        assert!(!AUTO_NEXT_GOAL.load(Ordering::Relaxed));
    }
}
