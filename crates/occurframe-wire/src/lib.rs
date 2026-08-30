//! Typed, language-neutral wire contracts for the Occurframe oracle.
//!
//! This crate deliberately contains no recurrence parsing or evaluation, timezone
//! resolution, subprocess management, or scoring logic.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The runner protocol version implemented by this authority layer.
pub const RUNNER_PROTOCOL_VERSION: &str = "2.0";
/// The official per-case certification budget in milliseconds.
pub const OFFICIAL_BUDGET_MS: u64 = 8_000;

/// A stable, permanent, versioned cron dialect identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DialectId(pub String);

/// The six corpus classifications. Classification and expectation mode are independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Classification {
    Normative,
    PolicyDependent,
    DialectDependent,
    AmbiguousStandard,
    KnownDivergence,
    Invalid,
}

/// Lifecycle state of an authored vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Active,
    Retired,
}

/// A reference to reusable evidence metadata in `registry/sources.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormativeEvidence {
    pub source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A language-neutral semantic-profile value. Floating point is intentionally excluded.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SemanticValue {
    Text(String),
    Integer(i64),
    Boolean(bool),
}

/// A named expected behavior under a particular semantic profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedCase {
    pub label: String,
    #[serde(default)]
    pub when: BTreeMap<String, SemanticValue>,
    /// `null` means the expected native outcome is deliberate rejection.
    pub occurrences: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Normative expectation modes. These modes are not inferred from classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum Expectation {
    Single {
        occurrences: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    Reject {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error_class: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    PerPolicy {
        cases: Vec<ExpectedCase>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    PerDialect {
        cases: Vec<ExpectedCase>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    Admissible {
        cases: Vec<ExpectedCase>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    Open {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
}

/// An authored RC2 conformance vector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vector {
    pub schema_version: String,
    pub corpus_version: String,
    pub id: String,
    pub family: String,
    pub title: String,
    pub kind: String,
    pub operation: String,
    pub input: Value,
    pub context: Value,
    pub classification: Classification,
    pub semantic_axes: Vec<String>,
    pub normative_evidence: Vec<NormativeEvidence>,
    pub expectation: Expectation,
    pub rationale: String,
    pub tags: Vec<String>,
    pub lifecycle: Lifecycle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersession: Option<Supersession>,
}

/// Explicit stable-ID supersession information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Supersession {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    pub reason: String,
}

/// Exact, bounded, unknown, or fingerprint-derived timezone database provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "release_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TzdbRelease {
    Exact {
        release: String,
    },
    Bounded {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_inclusive: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_inclusive: Option<String>,
    },
    Unknown,
}

/// Normalized tzdb provenance; source and confidence are mechanically distinct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TzdbProvenance {
    pub source: String,
    #[serde(flatten)]
    pub release: TzdbRelease,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

/// An implementation or runtime identity used in protocol messages and observations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentIdentity {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}

/// Language/runtime identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeIdentity {
    pub language: String,
    pub runtime: String,
    pub version: String,
}

/// Runner-to-authority protocol greeting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelloMessage {
    pub protocol_version: String,
    pub runner: ComponentIdentity,
    pub engine: ComponentIdentity,
    pub runtime: RuntimeIdentity,
    pub capabilities: Vec<String>,
    pub dialect_ids: Vec<DialectId>,
    pub semantic_profile_claims: BTreeMap<String, SemanticValue>,
    pub tzdb_provenance: TzdbProvenance,
}

/// Authority-to-runner case message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseMessage {
    pub protocol_version: String,
    pub request_id: String,
    pub vector: Vector,
    pub budget_ms: u64,
}

/// Runner acknowledgement emitted immediately before the native engine operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartedMessage {
    pub protocol_version: String,
    pub request_id: String,
}

/// The five and only five native-engine terminal outcomes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum EngineOutcome {
    Occurrences { occurrences: Vec<String> },
    Accepted,
    Rejection { diagnostic: Diagnostic },
    Unsupported { diagnostic: Diagnostic },
    EngineError { diagnostic: Diagnostic },
}

/// Orthogonal warning or error detail. Diagnostics do not redefine outcome status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Runner result following a `started` acknowledgement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultMessage {
    pub protocol_version: String,
    pub request_id: String,
    pub outcome: EngineOutcome,
    #[serde(default)]
    pub warnings: Vec<Diagnostic>,
}

/// Top-level NDJSON runner message union.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "message", rename_all = "snake_case", deny_unknown_fields)]
pub enum RunnerMessage {
    Hello(HelloMessage),
    Case(CaseMessage),
    Started(StartedMessage),
    Result(ResultMessage),
}

/// Execution-layer status. Infrastructure and timeout are not engine outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Completed,
    Timeout,
    RunnerFailure,
}

/// A normalized, portable engine observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedObservation {
    pub protocol_version: String,
    pub corpus_version: String,
    pub vector_id: String,
    pub runner: ComponentIdentity,
    pub engine: ComponentIdentity,
    pub runtime: RuntimeIdentity,
    pub dialect_ids: Vec<DialectId>,
    pub semantic_profile_claims: BTreeMap<String, SemanticValue>,
    pub tzdb_provenance: TzdbProvenance,
    pub execution_status: ExecutionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_outcome: Option<EngineOutcome>,
    #[serde(default)]
    pub warnings: Vec<Diagnostic>,
}

/// Scoring status, preserving conformance, unsupported, timeout, and infrastructure domains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictStatus {
    Conformant,
    ConformantAdmissible,
    NonConformant,
    Unsupported,
    Timeout,
    InfrastructureFailure,
    RecordedUnscored,
}

/// Deterministic scorer output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConformanceVerdict {
    pub status: VerdictStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_case: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<Diagnostic>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tzdb_release_forms_are_mechanically_distinct() {
        let exact = TzdbProvenance {
            source: "package".into(),
            release: TzdbRelease::Exact {
                release: "2026a".into(),
            },
            fingerprint: None,
        };
        let bounded = TzdbProvenance {
            source: "runtime ICU".into(),
            release: TzdbRelease::Bounded {
                min_inclusive: None,
                max_inclusive: Some("2026a".into()),
            },
            fingerprint: Some("icu-vancouver-fold".into()),
        };
        let unknown = TzdbProvenance {
            source: "system zoneinfo".into(),
            release: TzdbRelease::Unknown,
            fingerprint: None,
        };

        assert_ne!(exact, bounded);
        assert_ne!(bounded, unknown);
        assert_ne!(exact, unknown);
    }

    #[test]
    fn occurrence_order_is_preserved() {
        let outcome = EngineOutcome::Occurrences {
            occurrences: vec!["later".into(), "earlier".into(), "earlier".into()],
        };
        let encoded = serde_json::to_value(&outcome).expect("serialize outcome");
        assert_eq!(
            encoded["occurrences"],
            serde_json::json!(["later", "earlier", "earlier"])
        );
    }

    #[test]
    fn tzdb_provenance_round_trips_through_wire_json() {
        for provenance in [
            TzdbProvenance {
                source: "package".into(),
                release: TzdbRelease::Exact {
                    release: "2026a".into(),
                },
                fingerprint: None,
            },
            TzdbProvenance {
                source: "runtime ICU".into(),
                release: TzdbRelease::Bounded {
                    min_inclusive: None,
                    max_inclusive: Some("2026a".into()),
                },
                fingerprint: Some("fixture".into()),
            },
            TzdbProvenance {
                source: "system zoneinfo".into(),
                release: TzdbRelease::Unknown,
                fingerprint: None,
            },
        ] {
            let json = serde_json::to_value(&provenance).expect("serialize provenance");
            let decoded: TzdbProvenance =
                serde_json::from_value(json).expect("deserialize provenance");
            assert_eq!(decoded, provenance);
        }
    }

    #[test]
    fn all_five_engine_outcomes_are_distinct() {
        let diagnostic = Diagnostic {
            code: "test".into(),
            message: "test".into(),
            details: None,
        };
        let outcomes = [
            EngineOutcome::Occurrences {
                occurrences: Vec::new(),
            },
            EngineOutcome::Accepted,
            EngineOutcome::Rejection {
                diagnostic: diagnostic.clone(),
            },
            EngineOutcome::Unsupported {
                diagnostic: diagnostic.clone(),
            },
            EngineOutcome::EngineError { diagnostic },
        ];
        let kinds: Vec<_> = outcomes
            .iter()
            .map(|outcome| {
                serde_json::to_value(outcome).expect("serialize outcome")["type"]
                    .as_str()
                    .expect("outcome type")
                    .to_owned()
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "occurrences",
                "accepted",
                "rejection",
                "unsupported",
                "engine_error"
            ]
        );
    }
}
