//! Reports for different kinds of bugs.
//! ReportContent includes bug kind, possibility, diagnosis, and explanation.
//! The diagnosis for different kinds of bugs may be different.
//! e.g., doublelock diagnosis contains one deadlock diagnosis,
//！while conflictlock diagnosis contanis a vector of deadlock diagnosis.
//! Deadlock diagnosis consists of the first & second locks' type and span (a.k.a. src code location),
//! and **all** possible callchains from first to second lock.
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DeadlockDiagnosis {
    pub first_lock_type: String,
    pub first_lock_span: String,
    pub second_lock_type: String,
    pub second_lock_span: String,
    pub callchains: Vec<Vec<Vec<String>>>,
}

impl DeadlockDiagnosis {
    pub fn new(
        first_lock_type: String,
        first_lock_span: String,
        second_lock_type: String,
        second_lock_span: String,
        callchains: Vec<Vec<Vec<String>>>,
    ) -> Self {
        Self {
            first_lock_type,
            first_lock_span,
            second_lock_type,
            second_lock_span,
            callchains,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct ReportContent<D> {
    pub bug_kind: String,
    pub possibility: String,
    pub diagnosis: D,
    pub explanation: String,
}

impl<D: std::fmt::Debug> ReportContent<D> {
    pub fn new(bug_kind: String, possibility: String, diagnosis: D, explanation: String) -> Self {
        Self {
            bug_kind,
            possibility,
            diagnosis,
            explanation,
        }
    }
}

#[derive(Debug, Serialize)]
pub enum Report {
    DoubleLock(ReportContent<DeadlockDiagnosis>),
    ConflictLock(ReportContent<Vec<DeadlockDiagnosis>>),
}
