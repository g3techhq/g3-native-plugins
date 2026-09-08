//! Recording what a check did, once, for both audiences.
//!
//! Every result goes to three places: the system log for a terminal grepping
//! `G3TESTBED`, a scrolling log for someone reading the screen, and the status
//! map that drives the badges. Routing them through one type keeps the three
//! from drifting apart.

use crate::status::{Outcome, Results, Status};
use dioxus::prelude::*;

#[derive(Clone, Copy)]
pub struct Reporter {
    pub log: Signal<Vec<String>>,
    pub results: Results,
}

impl Reporter {
    pub fn record(&self, area: &str, status: Status, detail: impl AsRef<str>) {
        let detail = detail.as_ref().to_string();
        // The log keeps the shape a grep expects; "FAIL" stays shouty so it is
        // findable in a busy logcat.
        let marker = match status {
            Status::Pass => "ok",
            Status::Fail => "FAIL",
            Status::Waiting => "...",
        };
        let line = format!("G3TESTBED {area} {marker} {detail}");
        dioxus::logger::tracing::info!("{line}");

        let mut log = self.log;
        log.write().push(line);
        let mut results = self.results;
        results
            .write()
            .insert(area.to_string(), Outcome { status, detail });
    }

    /// Record `Ok` as a pass and `Err` as a failure, which is most checks.
    pub fn result<T: std::fmt::Debug>(&self, area: &str, result: Result<T, String>) {
        match result {
            Ok(value) => {
                let detail = format!("{value:?}");
                // `()` as a detail line tells a reader nothing, and a plain
                // message arrives from Debug wrapped in quotes it does not
                // need. Both are noise on a screen someone is reading.
                let detail = match detail.as_str() {
                    "()" => "accepted".to_string(),
                    _ => detail
                        .strip_prefix('"')
                        .and_then(|rest| rest.strip_suffix('"'))
                        .map(str::to_string)
                        .unwrap_or(detail),
                };
                self.record(area, Status::Pass, detail)
            }
            Err(error) => self.record(area, Status::Fail, error),
        }
    }

    /// Note that something was started and its answer is still to come.
    pub fn waiting(&self, area: &str, detail: impl AsRef<str>) {
        self.record(area, Status::Waiting, detail);
    }
}
