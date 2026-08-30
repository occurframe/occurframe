use occurframe_wire::{
    ConformanceVerdict, Diagnostic, EngineOutcome, ExecutionStatus, Expectation, ExpectedCase,
    NormalizedObservation, VerdictStatus,
};

/// Score one normalized observation against its authored expectation.
#[must_use]
pub fn score(expectation: &Expectation, observation: &NormalizedObservation) -> ConformanceVerdict {
    match observation.execution_status {
        ExecutionStatus::Timeout => return verdict(VerdictStatus::Timeout, None, None),
        ExecutionStatus::RunnerFailure => {
            return verdict(VerdictStatus::InfrastructureFailure, None, None);
        }
        ExecutionStatus::Completed => {}
    }

    let Some(outcome) = observation.engine_outcome.as_ref() else {
        return verdict(
            VerdictStatus::InfrastructureFailure,
            None,
            Some(Diagnostic {
                code: "missing_engine_outcome".into(),
                message: "completed execution omitted its engine outcome".into(),
                details: None,
            }),
        );
    };

    if matches!(outcome, EngineOutcome::Unsupported { .. }) {
        return verdict(VerdictStatus::Unsupported, None, None);
    }

    match expectation {
        Expectation::Open { .. } => verdict(VerdictStatus::RecordedUnscored, None, None),
        Expectation::Single { occurrences, .. } => {
            compare_occurrences(occurrences, outcome, VerdictStatus::Conformant, None)
        }
        Expectation::Reject { .. } => {
            if matches!(outcome, EngineOutcome::Rejection { .. }) {
                verdict(VerdictStatus::Conformant, None, None)
            } else {
                verdict(VerdictStatus::NonConformant, None, None)
            }
        }
        Expectation::Admissible { cases, .. } => {
            for case in cases {
                if case_matches_outcome(case, outcome) {
                    return verdict(
                        VerdictStatus::ConformantAdmissible,
                        Some(case.label.clone()),
                        None,
                    );
                }
            }
            verdict(VerdictStatus::NonConformant, None, None)
        }
        Expectation::PerPolicy { cases, .. } | Expectation::PerDialect { cases, .. } => {
            let matching_cases: Vec<_> = cases
                .iter()
                .filter(|case| {
                    case.when.iter().all(|(axis, expected)| {
                        observation.semantic_profile_claims.get(axis) == Some(expected)
                    })
                })
                .collect();
            if matching_cases.len() != 1 {
                return verdict(
                    VerdictStatus::NonConformant,
                    None,
                    Some(Diagnostic {
                        code: "unresolved_declared_profile".into(),
                        message:
                            "declared semantic profile selected zero or multiple expected cases"
                                .into(),
                        details: None,
                    }),
                );
            }
            let selected = matching_cases[0];
            if case_matches_outcome(selected, outcome) {
                verdict(
                    VerdictStatus::Conformant,
                    Some(selected.label.clone()),
                    None,
                )
            } else {
                verdict(
                    VerdictStatus::NonConformant,
                    Some(selected.label.clone()),
                    None,
                )
            }
        }
    }
}

fn compare_occurrences(
    expected: &[String],
    outcome: &EngineOutcome,
    success: VerdictStatus,
    matched_case: Option<String>,
) -> ConformanceVerdict {
    if let EngineOutcome::Occurrences { occurrences } = outcome {
        if occurrences == expected {
            return verdict(success, matched_case, None);
        }
    }
    verdict(VerdictStatus::NonConformant, matched_case, None)
}

fn case_matches_outcome(case: &ExpectedCase, outcome: &EngineOutcome) -> bool {
    match (&case.occurrences, outcome) {
        (Some(expected), EngineOutcome::Occurrences { occurrences }) => expected == occurrences,
        (None, EngineOutcome::Rejection { .. }) => true,
        _ => false,
    }
}

fn verdict(
    status: VerdictStatus,
    matched_case: Option<String>,
    diagnostic: Option<Diagnostic>,
) -> ConformanceVerdict {
    ConformanceVerdict {
        status,
        matched_case,
        diagnostic,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use occurframe_wire::{
        ComponentIdentity, DialectId, RuntimeIdentity, SemanticValue, TzdbProvenance, TzdbRelease,
    };

    use super::*;

    fn diagnostic() -> Diagnostic {
        Diagnostic {
            code: "test".into(),
            message: "test".into(),
            details: None,
        }
    }

    fn observation(
        status: ExecutionStatus,
        outcome: Option<EngineOutcome>,
    ) -> NormalizedObservation {
        NormalizedObservation {
            protocol_version: "2.0".into(),
            corpus_version: "1.0.0-rc2".into(),
            vector_id: "TEST-001".into(),
            runner: ComponentIdentity {
                name: "runner".into(),
                version: "1".into(),
                provenance: None,
            },
            engine: ComponentIdentity {
                name: "engine".into(),
                version: "1".into(),
                provenance: Some("sha256:test".into()),
            },
            runtime: RuntimeIdentity {
                language: "test".into(),
                runtime: "test".into(),
                version: "1".into(),
            },
            dialect_ids: vec![DialectId("cron.test@1".into())],
            semantic_profile_claims: BTreeMap::new(),
            tzdb_provenance: TzdbProvenance {
                source: "system zoneinfo".into(),
                release: TzdbRelease::Unknown,
                fingerprint: None,
            },
            execution_status: status,
            engine_outcome: outcome,
            warnings: Vec::new(),
        }
    }

    fn single() -> Expectation {
        Expectation::Single {
            occurrences: vec!["b".into(), "a".into(), "a".into()],
            note: None,
        }
    }

    fn reject() -> Expectation {
        Expectation::Reject {
            error_class: Some("parse".into()),
            note: None,
        }
    }

    #[test]
    fn scoring_truth_table() {
        let rejection = EngineOutcome::Rejection {
            diagnostic: diagnostic(),
        };
        let engine_error = EngineOutcome::EngineError {
            diagnostic: diagnostic(),
        };
        assert_eq!(
            score(
                &reject(),
                &observation(ExecutionStatus::Completed, Some(rejection))
            )
            .status,
            VerdictStatus::Conformant
        );
        assert_eq!(
            score(
                &reject(),
                &observation(ExecutionStatus::Completed, Some(engine_error))
            )
            .status,
            VerdictStatus::NonConformant
        );
        assert_eq!(
            score(
                &reject(),
                &observation(ExecutionStatus::RunnerFailure, None)
            )
            .status,
            VerdictStatus::InfrastructureFailure
        );
        assert_eq!(
            score(&reject(), &observation(ExecutionStatus::Timeout, None)).status,
            VerdictStatus::Timeout
        );
        assert_eq!(
            score(
                &single(),
                &observation(
                    ExecutionStatus::Completed,
                    Some(EngineOutcome::Unsupported {
                        diagnostic: diagnostic()
                    })
                )
            )
            .status,
            VerdictStatus::Unsupported
        );
        assert_eq!(
            score(
                &single(),
                &observation(ExecutionStatus::Completed, Some(EngineOutcome::Accepted))
            )
            .status,
            VerdictStatus::NonConformant
        );
    }

    #[test]
    fn exact_equality_preserves_order_duplicates_and_empty_output() {
        let matching = EngineOutcome::Occurrences {
            occurrences: vec!["b".into(), "a".into(), "a".into()],
        };
        assert_eq!(
            score(
                &single(),
                &observation(ExecutionStatus::Completed, Some(matching))
            )
            .status,
            VerdictStatus::Conformant
        );
        let reordered = EngineOutcome::Occurrences {
            occurrences: vec!["a".into(), "a".into(), "b".into()],
        };
        assert_eq!(
            score(
                &single(),
                &observation(ExecutionStatus::Completed, Some(reordered))
            )
            .status,
            VerdictStatus::NonConformant
        );
        let empty = Expectation::Single {
            occurrences: vec![],
            note: None,
        };
        assert_eq!(
            score(
                &empty,
                &observation(
                    ExecutionStatus::Completed,
                    Some(EngineOutcome::Occurrences {
                        occurrences: vec![]
                    })
                )
            )
            .status,
            VerdictStatus::Conformant
        );
    }

    #[test]
    fn warnings_are_orthogonal_to_success() {
        let mut observed = observation(
            ExecutionStatus::Completed,
            Some(EngineOutcome::Occurrences {
                occurrences: vec!["b".into(), "a".into(), "a".into()],
            }),
        );
        observed.warnings.push(diagnostic());
        assert_eq!(
            score(&single(), &observed).status,
            VerdictStatus::Conformant
        );
    }

    #[test]
    fn admissible_match_and_open_are_explicit() {
        let expectation = Expectation::Admissible {
            cases: vec![ExpectedCase {
                label: "one".into(),
                when: BTreeMap::new(),
                occurrences: Some(vec!["x".into()]),
                note: None,
            }],
            note: None,
        };
        let observed = observation(
            ExecutionStatus::Completed,
            Some(EngineOutcome::Occurrences {
                occurrences: vec!["x".into()],
            }),
        );
        let result = score(&expectation, &observed);
        assert_eq!(result.status, VerdictStatus::ConformantAdmissible);
        assert_eq!(result.matched_case.as_deref(), Some("one"));

        assert_eq!(
            score(&Expectation::Open { note: None }, &observed).status,
            VerdictStatus::RecordedUnscored
        );
    }

    #[test]
    fn declared_profile_cannot_pass_under_another_case() {
        let expectation = Expectation::PerDialect {
            cases: vec![
                ExpectedCase {
                    label: "A".into(),
                    when: BTreeMap::from([(
                        "cron.dom_dow".into(),
                        SemanticValue::Text("or".into()),
                    )]),
                    occurrences: Some(vec!["or-result".into()]),
                    note: None,
                },
                ExpectedCase {
                    label: "B".into(),
                    when: BTreeMap::from([(
                        "cron.dom_dow".into(),
                        SemanticValue::Text("and".into()),
                    )]),
                    occurrences: Some(vec!["and-result".into()]),
                    note: None,
                },
            ],
            note: None,
        };
        let mut observed = observation(
            ExecutionStatus::Completed,
            Some(EngineOutcome::Occurrences {
                occurrences: vec!["and-result".into()],
            }),
        );
        observed
            .semantic_profile_claims
            .insert("cron.dom_dow".into(), SemanticValue::Text("or".into()));
        assert_eq!(
            score(&expectation, &observed).status,
            VerdictStatus::NonConformant
        );
    }
}
