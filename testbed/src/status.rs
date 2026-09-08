//! The per-plugin result each check reports, and how it looks on screen.
//!
//! The test bed is read two ways: by a person watching it, and by a terminal
//! grepping logcat. Both want the same facts, so a check records its outcome
//! here once and the two views render it differently.

use dioxus::prelude::*;
use std::collections::BTreeMap;

/// How a single check turned out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    /// Started, waiting on the platform to answer.
    Waiting,
    /// Worked.
    Pass,
    /// Did not work.
    Fail,
}

impl Status {
    pub fn class(self) -> &'static str {
        match self {
            Status::Waiting => "waiting",
            Status::Pass => "pass",
            Status::Fail => "fail",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::Waiting => "waiting",
            Status::Pass => "pass",
            Status::Fail => "fail",
        }
    }
}

/// The latest result for one check.
#[derive(Clone, PartialEq, Debug)]
pub struct Outcome {
    pub status: Status,
    pub detail: String,
}

/// Every check's latest result, keyed by the same name used in the log.
pub type Results = Signal<BTreeMap<String, Outcome>>;

/// The badge and last line for one check, looked up by name.
#[component]
pub fn Check(area: String, label: String) -> Element {
    let results = use_context::<Results>();
    let outcome = results.read().get(&area).cloned();
    let (class, status, detail) = match &outcome {
        Some(outcome) => (
            outcome.status.class(),
            outcome.status.label(),
            outcome.detail.clone(),
        ),
        // Untried is not a failure, and should not look like one.
        None => ("idle", "not run", String::new()),
    };
    rsx! {
        div { class: "check",
            div { class: "check-head",
                span { class: "check-name", "{label}" }
                span { class: "badge {class}", "{status}" }
            }
            if !detail.is_empty() {
                div { class: "check-detail", "{detail}" }
            }
        }
    }
}

/// The pass/fail tally across everything tried so far.
#[component]
pub fn Summary() -> Element {
    let results = use_context::<Results>();
    let (passed, failed, waiting) =
        results
            .read()
            .values()
            .fold((0, 0, 0), |(pass, fail, wait), outcome| {
                match outcome.status {
                    Status::Pass => (pass + 1, fail, wait),
                    Status::Fail => (pass, fail + 1, wait),
                    Status::Waiting => (pass, fail, wait + 1),
                }
            });
    rsx! {
        div { class: "summary",
            span { class: "tally pass", "{passed} passing" }
            span {
                // A zero next to "failing" in red reads as alarming even when
                // it is the good outcome, so it stays neutral until it is not.
                class: if failed > 0 { "tally fail" } else { "tally idle" },
                "{failed} failing"
            }
            if waiting > 0 {
                span { class: "tally waiting", "{waiting} waiting" }
            }
        }
    }
}
