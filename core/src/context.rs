//! Execution-context facts: CI, interactivity, and actor attribution.
//!
//! Actor is never guessed from heuristics. It is `agent` only when an
//! explicit marker says so: the generic `SECCHI_ANALYTICS_ACTOR=agent`
//! override, or a well-known environment variable an agent framework sets
//! for exactly this purpose.

use std::io::IsTerminal;

use crate::event::Actor;

/// Environment facts attached to every event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionContext {
    pub actor: Actor,
    pub agent_name: Option<String>,
    pub agent_session_id: Option<String>,
    pub ci: bool,
    pub interactive: bool,
}

/// Environment variables that indicate a CI run when set to anything
/// non-empty.
const CI_MARKERS: &[&str] = &[
    "CI",
    "GITHUB_ACTIONS",
    "GITLAB_CI",
    "BUILDKITE",
    "CIRCLECI",
    "TRAVIS",
    "JENKINS_URL",
    "TEAMCITY_VERSION",
];

/// Detect the current process context from the real environment and TTYs.
#[must_use]
pub fn detect() -> ExecutionContext {
    let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    detect_from(|name| std::env::var(name).ok(), interactive)
}

/// Detection against an injectable environment — the testable core of
/// [`detect`].
pub fn detect_from<F>(env: F, interactive: bool) -> ExecutionContext
where
    F: Fn(&str) -> Option<String>,
{
    let non_empty = |name: &str| env(name).filter(|value| !value.is_empty());

    let ci = CI_MARKERS.iter().any(|marker| non_empty(marker).is_some());

    let (actor, agent_name) = if let Some(explicit) = non_empty("SECCHI_ANALYTICS_ACTOR") {
        if explicit == "agent" {
            (Actor::Agent, non_empty("SECCHI_ANALYTICS_AGENT_NAME"))
        } else {
            (Actor::Human, None)
        }
    } else if non_empty("CLAUDECODE").is_some() {
        (Actor::Agent, Some("claude-code".to_owned()))
    } else {
        (Actor::Human, None)
    };

    let agent_session_id = match actor {
        Actor::Agent => non_empty("SECCHI_ANALYTICS_AGENT_SESSION"),
        Actor::Human => None,
    };

    ExecutionContext {
        actor,
        agent_name,
        agent_session_id,
        ci,
        interactive,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn defaults_to_human_outside_ci() {
        let context = detect_from(env_of(&[]), true);
        assert_eq!(
            context,
            ExecutionContext {
                actor: Actor::Human,
                agent_name: None,
                agent_session_id: None,
                ci: false,
                interactive: true,
            }
        );
    }

    #[test]
    fn ci_markers_are_detected_but_empty_values_ignored() {
        assert!(detect_from(env_of(&[("GITHUB_ACTIONS", "true")]), false).ci);
        assert!(!detect_from(env_of(&[("CI", "")]), false).ci);
    }

    #[test]
    fn claude_code_marker_attributes_the_agent() {
        let context = detect_from(env_of(&[("CLAUDECODE", "1")]), false);
        assert_eq!(context.actor, Actor::Agent);
        assert_eq!(context.agent_name.as_deref(), Some("claude-code"));
    }

    #[test]
    fn explicit_actor_override_wins() {
        let context = detect_from(
            env_of(&[
                ("SECCHI_ANALYTICS_ACTOR", "agent"),
                ("SECCHI_ANALYTICS_AGENT_NAME", "my-agent"),
                ("SECCHI_ANALYTICS_AGENT_SESSION", "sess-9"),
            ]),
            false,
        );
        assert_eq!(context.actor, Actor::Agent);
        assert_eq!(context.agent_name.as_deref(), Some("my-agent"));
        assert_eq!(context.agent_session_id.as_deref(), Some("sess-9"));

        // An explicit human override suppresses agent markers entirely.
        let context = detect_from(
            env_of(&[("SECCHI_ANALYTICS_ACTOR", "human"), ("CLAUDECODE", "1")]),
            false,
        );
        assert_eq!(context.actor, Actor::Human);
        assert_eq!(context.agent_name, None);
    }
}
