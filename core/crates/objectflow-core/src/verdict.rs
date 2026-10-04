//! What a request is answered with: one of the library's verdicts
//! (`library-api.md` §4), matched exhaustively, a refusal a value and never
//! an error.

use crate::model::Remedy;

/// A clause marked to warn, which failed and refused nothing (ADR-0111).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flag {
    pub clause: String,
    pub remedy: Option<Remedy>,
}

/// An invariant that fails, on the object it fails on (ADR-0151).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvariantFailure {
    pub object_id: String,
    pub type_: String,
    pub invariant: String,
    /// `None` where the core cannot tell, as the runner cannot.
    pub remedy: Option<Remedy>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Satisfied {
        object_id: String,
        version: i64,
        /// The positions of every event it appended, the parent's first.
        events: Vec<i64>,
        flags: Vec<Flag>,
    },
    Unsatisfied {
        clause: String,
        remedy: Option<Remedy>,
    },
    NotFound,
    Unavailable {
        state: String,
    },
    UnknownTransition {
        transition: String,
    },
    InvalidInput {
        inputs: Vec<String>,
    },
    NotRequestable {
        parents: Vec<String>,
    },
    InvariantViolated {
        failures: Vec<InvariantFailure>,
        remedy: Option<Remedy>,
    },
}

impl Verdict {
    pub fn kind(&self) -> &'static str {
        match self {
            Verdict::Satisfied { .. } => "satisfied",
            Verdict::Unsatisfied { .. } => "unsatisfied",
            Verdict::NotFound => "not_found",
            Verdict::Unavailable { .. } => "unavailable",
            Verdict::UnknownTransition { .. } => "unknown_transition",
            Verdict::InvalidInput { .. } => "invalid_input",
            Verdict::NotRequestable { .. } => "not_requestable",
            Verdict::InvariantViolated { .. } => "invariant_violated",
        }
    }

    /// Its remedy class (`DESIGN.md` §5.5): every refusal has one.
    pub fn remedy(&self) -> Option<Remedy> {
        match self {
            Verdict::Satisfied { .. } => None,
            Verdict::Unsatisfied { remedy, .. } | Verdict::InvariantViolated { remedy, .. } => {
                *remedy
            }
            Verdict::NotFound | Verdict::Unavailable { .. } | Verdict::NotRequestable { .. } => {
                Some(Remedy::UnreachableFromHere)
            }
            Verdict::UnknownTransition { .. } | Verdict::InvalidInput { .. } => {
                Some(Remedy::SelfServiceable)
            }
        }
    }
}
