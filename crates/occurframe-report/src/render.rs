use std::{collections::BTreeMap, fmt::Write};

use occurframe_conformance::canonical_json;
use occurframe_wire::TzdbRelease;

use crate::{
    Result,
    model::{
        CertificationManifest, DifferentialMatrix, DivergenceKind, ReconciliationReport,
        outcome_name, verdict_name,
    },
};

pub(crate) fn public_release_markdown(
    matrix: &DifferentialMatrix,
    manifest: &CertificationManifest,
    provenance_blocked_builds: &[String],
) -> Result<Vec<u8>> {
    let summary = &matrix.summary;
    let mut output = String::new();
    writeln!(output, "# Occurframe RC2 Differential Evidence")?;
    writeln!(output)?;
    writeln!(
        output,
        "This is candidate evidence for Occurframe `{}` over corpus `{}` using runner protocol `{}`. The corpus is the normative authority; observations and this report are derived evidence.",
        manifest.certification_profile_version,
        matrix.corpus_version,
        matrix.runner_protocol_version
    )?;
    writeln!(output)?;
    writeln!(
        output,
        "Only **{}/{} historical builds were reproducible**. The provenance-blocked builds are: {}. They were not replaced with newer dependencies or engine versions.",
        summary.reproducible_builds,
        summary.configured_builds,
        provenance_blocked_builds
            .iter()
            .map(|build| format!("`{build}`"))
            .collect::<Vec<_>>()
            .join(", ")
    )?;
    writeln!(output)?;
    writeln!(output, "## Certified population")?;
    writeln!(output)?;
    writeln!(output, "- Vectors: {}", summary.vectors)?;
    writeln!(
        output,
        "- Complete observations: {} / {}",
        summary.actual_observations, summary.expected_observations
    )?;
    writeln!(
        output,
        "- Semantic-divergence vectors: {}",
        summary.semantic_divergence_vectors
    )?;
    writeln!(
        output,
        "- Normative-violation vectors: {}",
        summary.normative_violation_vectors
    )?;
    writeln!(
        output,
        "- Documented policy-difference vectors: {}",
        summary.documented_policy_difference_vectors
    )?;
    writeln!(
        output,
        "- Documented dialect-difference vectors: {}",
        summary.documented_dialect_difference_vectors
    )?;
    writeln!(
        output,
        "- Ambiguous-standard vectors: {} ({} with multiple measured answers)",
        summary.ambiguous_standard_vectors, summary.ambiguous_standard_divergent_vectors
    )?;
    writeln!(
        output,
        "- TZDB-dependent difference vectors: {}",
        summary.tzdb_difference_vectors
    )?;
    writeln!(output)?;
    writeln!(output, "## Execution outcomes")?;
    writeln!(output)?;
    write_count_table(
        &mut output,
        "Measured outcome counts",
        &summary.outcome_counts,
    )?;
    writeln!(
        output,
        "The evidence includes **{} timeout** and **{} engine errors**; neither is hidden or converted into a semantic answer. Unsupported cells ({}) are also retained explicitly.",
        count(&summary.outcome_counts, "timeout"),
        count(&summary.outcome_counts, "engine_error"),
        count(&summary.outcome_counts, "unsupported")
    )?;
    writeln!(output)?;
    writeln!(output, "## How to interpret divergence")?;
    writeln!(output)?;
    writeln!(
        output,
        "A divergence is not automatically a defect. The RC2 authority distinguishes normative answers, named policy differences, named dialect differences, standard ambiguity, known implementation divergence, and TZDB-provenance differences. Unsupported, engine errors, timeouts, and runner failures are excluded from semantic-answer grouping."
    )?;
    writeln!(output)?;
    writeln!(
        output,
        "RC2's measured divergence count is not required to reproduce Research II's earlier `157/184` headline. The engine population, protocol outcome taxonomy, provenance requirements, and scoring are more precise in protocol v2."
    )?;
    writeln!(output)?;
    writeln!(
        output,
        "No engine ranking or invented overall quality score is presented. See `matrix.json` for every cell, its normalized outcome, conformance verdict, matched case, warnings, and provenance reference."
    )?;
    Ok(output.into_bytes())
}

pub(crate) fn matrix_csv(matrix: &DifferentialMatrix) -> Result<Vec<u8>> {
    let vector_by_id: BTreeMap<_, _> = matrix
        .vectors
        .iter()
        .map(|vector| (vector.vector_id.as_str(), vector))
        .collect();
    let provenance: BTreeMap<_, _> = matrix
        .provenance
        .iter()
        .map(|item| (item.build_id.as_str(), item))
        .collect();
    let mut output = String::from(
        "vector_id,build_id,family,classification,operation,execution_status,outcome,verdict,matched_case,semantic_answer_id,warning_count,dialect_ids,semantic_profile_claims,tzdb_source,tzdb_release,answer\n",
    );
    for cell in &matrix.cells {
        let vector = vector_by_id[cell.vector_id.as_str()];
        let build = provenance[cell.build_id.as_str()];
        let values = [
            cell.vector_id.clone(),
            cell.build_id.clone(),
            vector.family.clone(),
            super::matrix::classification_name(vector.classification).into(),
            vector.operation.clone(),
            serde_json::to_value(cell.execution_status)?
                .as_str()
                .unwrap_or_default()
                .into(),
            outcome_name(cell.outcome_kind).into(),
            verdict_name(cell.verdict.status).into(),
            cell.verdict.matched_case.clone().unwrap_or_default(),
            cell.semantic_answer_id.clone().unwrap_or_default(),
            cell.warnings.len().to_string(),
            canonical_string(&build.dialect_ids)?,
            canonical_string(&build.semantic_profile_claims)?,
            build.tzdb_provenance.source.clone(),
            tzdb_release(&build.tzdb_provenance.release),
            cell.engine_outcome
                .as_ref()
                .map(canonical_string)
                .transpose()?
                .unwrap_or_default(),
        ];
        output.push_str(
            &values
                .iter()
                .map(|value| csv_field(value))
                .collect::<Vec<_>>()
                .join(","),
        );
        output.push('\n');
    }
    Ok(output.into_bytes())
}

pub(crate) fn differential_markdown(matrix: &DifferentialMatrix) -> Result<Vec<u8>> {
    let summary = &matrix.summary;
    let mut output = String::new();
    writeln!(output, "# RC2 Differential Certification")?;
    writeln!(output)?;
    writeln!(
        output,
        "This report is a derived view of protocol-v2 observations. The RC2 corpus remains normative authority; this report neither changes expectations nor ranks engines."
    )?;
    writeln!(output)?;
    writeln!(output, "## Certification identity and coverage")?;
    writeln!(output)?;
    writeln!(output, "- Certification: `{}`", matrix.certification_id)?;
    writeln!(
        output,
        "- Certification profile: `{}`",
        matrix.certification_profile_version
    )?;
    writeln!(
        output,
        "- Tooling source SHA: `{}`",
        matrix.tooling_source_sha
    )?;
    writeln!(
        output,
        "- Corpus: `{}` at `{}`",
        matrix.corpus_version, matrix.corpus_sha
    )?;
    writeln!(
        output,
        "- Runner protocol: `{}`",
        matrix.runner_protocol_version
    )?;
    writeln!(
        output,
        "- Canonical platform: `{}`",
        matrix.canonical_platform
    )?;
    writeln!(
        output,
        "- Actual observed environment: `environment.json` (the platform label alone does not pin runtimes or TZDB)"
    )?;
    writeln!(output, "- Configured builds: {}", summary.configured_builds)?;
    writeln!(
        output,
        "- Reproducible measured builds: {}",
        summary.reproducible_builds
    )?;
    writeln!(
        output,
        "- Unreproducible builds: {}",
        summary.unreproducible_builds
    )?;
    writeln!(output, "- Vectors: {}", summary.vectors)?;
    writeln!(
        output,
        "- Observation cells: {} / {}",
        summary.actual_observations, summary.expected_observations
    )?;
    writeln!(output)?;
    write_count_table(&mut output, "Outcome counts", &summary.outcome_counts)?;
    write_count_table(
        &mut output,
        "Conformance verdict counts",
        &summary.verdict_counts,
    )?;
    write_count_table(&mut output, "Family coverage", &summary.family_counts)?;
    write_count_table(
        &mut output,
        "Classification coverage",
        &summary.classification_counts,
    )?;

    writeln!(output, "## Differential summary")?;
    writeln!(output)?;
    writeln!(
        output,
        "- Semantic-divergence vectors: {}",
        summary.semantic_divergence_vectors
    )?;
    writeln!(
        output,
        "- Normative-violation vectors: {}",
        summary.normative_violation_vectors
    )?;
    writeln!(
        output,
        "- Documented policy-difference vectors: {}",
        summary.documented_policy_difference_vectors
    )?;
    writeln!(
        output,
        "- Documented dialect-difference vectors: {}",
        summary.documented_dialect_difference_vectors
    )?;
    writeln!(
        output,
        "- Named policy answers accepted by the scorer: {} cells across {} vectors",
        summary.named_policy_conformant_cells, summary.policy_vectors_with_named_conformant_answer
    )?;
    writeln!(
        output,
        "- Named dialect answers accepted by the scorer: {} cells across {} vectors",
        summary.named_dialect_conformant_cells,
        summary.dialect_vectors_with_named_conformant_answer
    )?;
    writeln!(
        output,
        "- Ambiguous-standard vectors: {} total; {} with multiple measured answers",
        summary.ambiguous_standard_vectors, summary.ambiguous_standard_divergent_vectors
    )?;
    writeln!(
        output,
        "- TZDB-dependent difference vectors: {}",
        summary.tzdb_difference_vectors
    )?;
    writeln!(
        output,
        "- Timeouts: {}",
        count(&summary.outcome_counts, "timeout")
    )?;
    writeln!(
        output,
        "- Engine errors: {}",
        count(&summary.outcome_counts, "engine_error")
    )?;
    writeln!(
        output,
        "- Unsupported cells: {}",
        count(&summary.outcome_counts, "unsupported")
    )?;
    writeln!(
        output,
        "- Runner failures: {}",
        count(&summary.outcome_counts, "runner_failure")
    )?;
    writeln!(output)?;
    writeln!(
        output,
        "Semantic divergence counts only occurrences, accepted parses, and deliberate rejections as answers. Unsupported, engine errors, timeouts, and runner failures are execution-pathology categories and are excluded from answer grouping."
    )?;
    writeln!(output)?;
    writeln!(
        output,
        "No aggregate engine ranking is computed. Any conformance denominator must state how unsupported and unscored cells are handled; this report therefore presents counts rather than an invented overall percentage."
    )?;
    writeln!(output)?;
    writeln!(output, "## Measured build provenance")?;
    writeln!(output)?;
    writeln!(
        output,
        "| Build | Runner | Engine | Runtime | Dialect/profile claims | TZDB provenance |"
    )?;
    writeln!(output, "|---|---|---|---|---|---|")?;
    for item in &matrix.provenance {
        let dialects = canonical_string(&item.dialect_ids)?;
        let profiles = canonical_string(&item.semantic_profile_claims)?;
        let claims = format!("dialects={dialects}; profiles={profiles}");
        let tzdb = canonical_string(&item.tzdb_provenance)?;
        writeln!(
            output,
            "| `{}` | `{}` `{}` | `{}` `{}` (`{}`) | `{}` `{}` | `{}` | `{}` |",
            markdown_cell(&item.build_id),
            markdown_cell(&item.runner.name),
            markdown_cell(&item.runner.version),
            markdown_cell(&item.engine.name),
            markdown_cell(&item.engine.version),
            markdown_cell(item.engine.provenance.as_deref().unwrap_or("unrecorded")),
            markdown_cell(&item.runtime.language),
            markdown_cell(&item.runtime.version),
            markdown_cell(&claims),
            markdown_cell(&tzdb),
        )?;
    }
    writeln!(output)?;
    writeln!(output, "## Semantic answer groups")?;
    writeln!(output)?;
    if matrix.semantic_divergences.is_empty() {
        writeln!(
            output,
            "No vector produced more than one completed semantic answer."
        )?;
    }
    for divergence in &matrix.semantic_divergences {
        writeln!(
            output,
            "### `{}` — `{}`",
            divergence.vector_id,
            divergence_name(divergence.kind)
        )?;
        writeln!(output)?;
        writeln!(
            output,
            "Classification: `{}`. Registered axes: {}.",
            super::matrix::classification_name(divergence.classification),
            if divergence.semantic_axes.is_empty() {
                "none".into()
            } else {
                divergence
                    .semantic_axes
                    .iter()
                    .map(|axis| format!("`{axis}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        )?;
        writeln!(output)?;
        for group in &divergence.answer_groups {
            writeln!(
                output,
                "- `{}`: {} build(s): {}. Verdicts: {}. Matched cases: {}. Answer: `{}`",
                group.answer_id,
                group.builds.len(),
                group.builds.join(", "),
                map_summary(&group.verdict_counts),
                if group.matched_cases.is_empty() {
                    "none".into()
                } else {
                    group.matched_cases.join(", ")
                },
                canonical_string(&group.answer)?
            )?;
        }
        writeln!(output)?;
    }
    Ok(output.into_bytes())
}

pub(crate) fn reconciliation_markdown(report: &ReconciliationReport) -> Result<Vec<u8>> {
    let mut output = String::new();
    writeln!(output, "# Phase II / RC2 Reconciliation")?;
    writeln!(output)?;
    writeln!(
        output,
        "Phase II RC1 remains historical evidence. This reconciliation does not rewrite RC1 outcomes, alter RC2 expectations, infer correctness from majority behavior, or use the Phase II `157/184` headline as an acceptance target."
    )?;
    writeln!(output)?;
    writeln!(
        output,
        "- Comparable protocol-v2 cells: {}",
        report.comparable_cells
    )?;
    writeln!(
        output,
        "- Observed differences classified: {}",
        report.classified_observed_differences
    )?;
    writeln!(
        output,
        "- Legacy ambiguous-error inventory: {}",
        report.legacy_ambiguous_error_inventory
    )?;
    writeln!(
        output,
        "- Builds not comparable because provenance prevented reproduction: {}",
        if report.not_comparable_builds.is_empty() {
            "none".into()
        } else {
            report.not_comparable_builds.join(", ")
        }
    )?;
    writeln!(output)?;
    write_count_table(
        &mut output,
        "Reconciliation categories",
        &report.category_counts,
    )?;
    writeln!(output, "## Unresolved differences")?;
    writeln!(output)?;
    if report.unresolved_differences.is_empty() {
        writeln!(output, "None.")?;
    } else {
        for cell in &report.unresolved_differences {
            writeln!(
                output,
                "- `{}` / `{}`: {}",
                cell.vector_id, cell.build_id, cell.evidence
            )?;
        }
    }
    writeln!(output)?;
    writeln!(
        output,
        "The machine-readable reconciliation retains every classified cell, including all RC1 generic `error` cells as `legacy_ambiguous_error` unless concrete cell-specific evidence exists."
    )?;
    Ok(output.into_bytes())
}

fn canonical_string<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(String::from_utf8(canonical_json(value)?)?)
}

fn markdown_cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace('`', "&#96;")
        .replace(['\r', '\n'], " ")
}

fn csv_field(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn tzdb_release(release: &TzdbRelease) -> String {
    match release {
        TzdbRelease::Exact { release } => format!("exact:{release}"),
        TzdbRelease::Bounded {
            min_inclusive,
            max_inclusive,
        } => format!(
            "bounded:{}..{}",
            min_inclusive.as_deref().unwrap_or(""),
            max_inclusive.as_deref().unwrap_or("")
        ),
        TzdbRelease::Unknown => "unknown".into(),
    }
}

fn write_count_table(
    output: &mut String,
    title: &str,
    counts: &BTreeMap<String, usize>,
) -> std::fmt::Result {
    writeln!(output, "## {title}")?;
    writeln!(output)?;
    writeln!(output, "| Category | Count |")?;
    writeln!(output, "|---|---:|")?;
    for (name, value) in counts {
        writeln!(output, "| `{name}` | {value} |")?;
    }
    writeln!(output)
}

fn count(counts: &BTreeMap<String, usize>, key: &str) -> usize {
    counts.get(key).copied().unwrap_or(0)
}

fn map_summary(counts: &BTreeMap<String, usize>) -> String {
    counts
        .iter()
        .map(|(name, count)| format!("{name}={count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn divergence_name(kind: DivergenceKind) -> &'static str {
    match kind {
        DivergenceKind::NormativeViolation => "normative violation",
        DivergenceKind::DocumentedPolicyDifference => "documented policy difference",
        DivergenceKind::DocumentedDialectDifference => "documented dialect difference",
        DivergenceKind::StandardAmbiguity => "standard ambiguity",
        DivergenceKind::KnownImplementationDivergence => "known implementation divergence",
        DivergenceKind::TzdbProvenanceDifference => "tzdb provenance difference",
    }
}
