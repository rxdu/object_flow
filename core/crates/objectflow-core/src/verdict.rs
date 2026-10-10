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

/// Why an input is not valid, beside each input `invalid_input` names
/// (`DESIGN.md` §5.5, ADR-0173 decision 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputReason {
    /// The transition takes no input of that name.
    UnknownInput,
    /// An input it requires, left out.
    Missing,
    /// A value of another kind than the input's type: another JSON kind,
    /// or a member of no enumeration it names.
    WrongType,
    /// A value of its kind not written in its notation: a time or a date
    /// that does not parse, an amount without its currency, more places
    /// than its scale, a string holding U+0000, an empty name.
    Malformed,
    /// An amount in another currency than the input's.
    WrongCurrency,
    /// A reference naming no object of the type it takes.
    NoSuchObject,
    /// A value its type cannot hold: an int past 64 bits, a decimal or an
    /// amount of more than 18 digits (ADR-0143).
    OutOfRange,
    /// A value of its type the transition does not take: an assertion's
    /// state or admission it does not list, a correction of another
    /// subject.
    NotAllowed,
}

impl InputReason {
    /// Its name on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            InputReason::UnknownInput => "unknown_input",
            InputReason::Missing => "missing",
            InputReason::WrongType => "wrong_type",
            InputReason::Malformed => "malformed",
            InputReason::WrongCurrency => "wrong_currency",
            InputReason::NoSuchObject => "no_such_object",
            InputReason::OutOfRange => "out_of_range",
            InputReason::NotAllowed => "not_allowed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Satisfied {
        object_id: String,
        version: i64,
        /// The positions of every event it appended, the parent's first.
        events: Vec<i64>,
        flags: Vec<Flag>,
        /// Returned for a retry of a request its actor's key applied before,
        /// the first result replayed (`DESIGN.md` §6 step 2, ADR-0041).
        replayed: bool,
    },
    /// The object moved since the caller read it: the version expected, and
    /// the one it is at (`library-api.md` §4, cause `expected_version`).
    Stale {
        expected: i64,
        actual: i64,
    },
    /// The store's write lock was held past the busy timeout and every
    /// retry, a race answered as a verdict, never a fault: `stale`, cause
    /// `retries_exhausted`, neither version (`library-api.md` §4,
    /// ADR-0090, ADR-0178).
    RetriesExhausted,
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
    /// Each input at fault, and why, `reasons[i]` being `inputs[i]`'s;
    /// built by `Verdict::invalid_input`, which keeps them in step.
    InvalidInput {
        inputs: Vec<String>,
        reasons: Vec<InputReason>,
    },
    /// A proposal of a transition not marked proposable (ADR-0173).
    NotProposable,
    NotRequestable {
        parents: Vec<String>,
    },
    InvariantViolated {
        failures: Vec<InvariantFailure>,
        remedy: Option<Remedy>,
    },
    /// A call or a creation an effect made was refused, which refuses the
    /// request (ADR-0122 decision 35).
    CallRefused {
        /// The effect step: `call <target>.<transition>` or `create
        /// <Type>.<transition>`.
        step: String,
        /// The object it targeted; none for a creation.
        object_id: Option<String>,
        /// That call's own refusal.
        verdict: Box<Verdict>,
        /// Its clause, qualified by the type it targeted, as an expectation
        /// names it (`flow-format.md` §11.4).
        clause: Option<String>,
    },
    /// A loop or a cascade would take more than its declared limit.
    OverLimit {
        loop_: String,
        bound: i64,
    },
}

impl Verdict {
    pub fn kind(&self) -> &'static str {
        match self {
            Verdict::Satisfied { .. } => "satisfied",
            Verdict::Stale { .. } | Verdict::RetriesExhausted => "stale",
            Verdict::Unsatisfied { .. } => "unsatisfied",
            Verdict::NotFound => "not_found",
            Verdict::Unavailable { .. } => "unavailable",
            Verdict::UnknownTransition { .. } => "unknown_transition",
            Verdict::InvalidInput { .. } => "invalid_input",
            Verdict::NotRequestable { .. } => "not_requestable",
            Verdict::NotProposable => "not_proposable",
            Verdict::InvariantViolated { .. } => "invariant_violated",
            Verdict::CallRefused { .. } => "call_refused",
            Verdict::OverLimit { .. } => "over_limit",
        }
    }

    /// The clause a refusal names, where it names one.
    pub fn clause(&self) -> Option<String> {
        match self {
            Verdict::Unsatisfied { clause, .. } => Some(clause.clone()),
            Verdict::InvariantViolated { failures, .. } => {
                failures.first().map(|f| f.invariant.clone())
            }
            Verdict::CallRefused { clause, .. } => clause.clone(),
            _ => None,
        }
    }

    /// An `invalid_input`, each input at fault with why (ADR-0173).
    pub fn invalid_input<I>(pairs: I) -> Verdict
    where
        I: IntoIterator<Item = (String, InputReason)>,
    {
        let (inputs, reasons) = pairs.into_iter().unzip();
        Verdict::InvalidInput { inputs, reasons }
    }

    /// Its remedy class (`DESIGN.md` §5.5): every refusal has one.
    pub fn remedy(&self) -> Option<Remedy> {
        match self {
            Verdict::Satisfied { .. } => None,
            Verdict::Unsatisfied { remedy, .. } | Verdict::InvariantViolated { remedy, .. } => {
                *remedy
            }
            Verdict::CallRefused { verdict, .. } => verdict.remedy(),
            Verdict::NotFound
            | Verdict::Unavailable { .. }
            | Verdict::NotRequestable { .. }
            | Verdict::OverLimit { .. } => Some(Remedy::UnreachableFromHere),
            // the race is over by the time it is sent again (ADR-0105)
            Verdict::RetriesExhausted => Some(Remedy::Temporal),
            // a moved version: re-read and send again (ADR-0105)
            // a transition not proposable: send it as a request, if its
            // caller may (ADR-0173)
            Verdict::UnknownTransition { .. }
            | Verdict::InvalidInput { .. }
            | Verdict::NotProposable
            | Verdict::Stale { .. } => Some(Remedy::SelfServiceable),
        }
    }
}
