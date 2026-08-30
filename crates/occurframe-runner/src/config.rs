use std::{collections::BTreeMap, fs, path::Path};

use occurframe_wire::{
    ComponentIdentity, DialectId, HelloMessage, RUNNER_PROTOCOL_VERSION, RuntimeIdentity,
    SemanticValue, TzdbProvenance, TzdbRelease,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Result};

/// Checked-in launch description. Relative executable and working-directory paths
/// are resolved from the tooling repository root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchConfig {
    pub program: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default = "default_working_directory")]
    pub working_directory: String,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}

fn default_working_directory() -> String {
    ".".into()
}

/// Mechanically checkable timezone-provenance release categories allowed for a
/// configured runtime acquisition method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TzdbReleaseKind {
    Exact,
    Bounded,
    Unknown,
}

/// Whether an exact historical engine build can currently be restored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReproducibilityStatus {
    Reproducible,
    Unreproducible,
}

/// Reproducible setup declaration for one immutable engine/configuration build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reproducibility {
    pub status: ReproducibilityStatus,
    pub setup: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// One launched process and its immutable protocol identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerBuild {
    pub build_id: String,
    pub protocol_version: String,
    pub runner: ComponentIdentity,
    pub engine: ComponentIdentity,
    pub language: String,
    pub runtime_name: String,
    pub runtime_requirement: String,
    pub launch: LaunchConfig,
    pub supported_operations: Vec<String>,
    pub dialect_ids: Vec<DialectId>,
    #[serde(default)]
    pub semantic_profile_claims: BTreeMap<String, SemanticValue>,
    pub tzdb_provenance_acquisition: String,
    pub tzdb_source: String,
    #[serde(default)]
    pub additional_tzdb_sources: Vec<String>,
    pub allowed_tzdb_release_kinds: Vec<TzdbReleaseKind>,
    pub fallback_tzdb_provenance: TzdbProvenance,
    pub representative_vectors: Vec<String>,
    pub reproducibility: Reproducibility,
    pub legacy_source: String,
}

impl RunnerBuild {
    /// A deterministic configured identity used only when a process fails before
    /// it can provide a trustworthy `hello`.
    #[must_use]
    pub fn fallback_hello(&self) -> HelloMessage {
        HelloMessage {
            protocol_version: RUNNER_PROTOCOL_VERSION.into(),
            runner: self.runner.clone(),
            engine: self.engine.clone(),
            runtime: RuntimeIdentity {
                language: self.language.clone(),
                runtime: self.runtime_name.clone(),
                version: self.runtime_requirement.clone(),
            },
            capabilities: self.supported_operations.clone(),
            dialect_ids: self.dialect_ids.clone(),
            semantic_profile_claims: self.semantic_profile_claims.clone(),
            tzdb_provenance: self.fallback_tzdb_provenance.clone(),
        }
    }

    pub(crate) fn validate_hello(&self, hello: &HelloMessage) -> std::result::Result<(), String> {
        if hello.protocol_version != RUNNER_PROTOCOL_VERSION
            || hello.protocol_version != self.protocol_version
        {
            return Err(format!(
                "protocol version mismatch: configured {}, observed {}",
                self.protocol_version, hello.protocol_version
            ));
        }
        if hello.runner != self.runner {
            return Err("runner identity differs from configuration".into());
        }
        if hello.engine != self.engine {
            return Err("engine identity/provenance differs from configuration".into());
        }
        if hello.runtime.language != self.language || hello.runtime.runtime != self.runtime_name {
            return Err("runtime language/name differs from configuration".into());
        }
        if hello.capabilities != self.supported_operations {
            return Err("capability declaration differs from configuration".into());
        }
        if hello.dialect_ids != self.dialect_ids {
            return Err("dialect declaration differs from configuration".into());
        }
        if hello.semantic_profile_claims != self.semantic_profile_claims {
            return Err("semantic profile declaration differs from configuration".into());
        }
        if hello.tzdb_provenance.source != self.tzdb_source
            && !self
                .additional_tzdb_sources
                .contains(&hello.tzdb_provenance.source)
        {
            return Err("tzdb source differs from configuration".into());
        }
        let release_kind = match hello.tzdb_provenance.release {
            TzdbRelease::Exact { .. } => TzdbReleaseKind::Exact,
            TzdbRelease::Bounded { .. } => TzdbReleaseKind::Bounded,
            TzdbRelease::Unknown => TzdbReleaseKind::Unknown,
        };
        if !self.allowed_tzdb_release_kinds.contains(&release_kind) {
            return Err(format!(
                "tzdb release kind {release_kind:?} is not configured"
            ));
        }
        Ok(())
    }
}

/// Versioned registry of migrated, immutable runner builds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerRegistry {
    pub schema_version: String,
    pub builds: Vec<RunnerBuild>,
}

impl RunnerRegistry {
    pub fn load(path: &Path) -> Result<Self> {
        let registry: Self = serde_json::from_slice(&fs::read(path)?)?;
        registry.validate()?;
        Ok(registry)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != "1.0.0" {
            return Err(Error::Configuration(format!(
                "unsupported runner registry schema {}",
                self.schema_version
            )));
        }
        let mut ids = std::collections::BTreeSet::new();
        for build in &self.builds {
            if !ids.insert(&build.build_id) {
                return Err(Error::Configuration(format!(
                    "duplicate build ID {}",
                    build.build_id
                )));
            }
            if build.protocol_version != RUNNER_PROTOCOL_VERSION {
                return Err(Error::Configuration(format!(
                    "{} configures protocol {}",
                    build.build_id, build.protocol_version
                )));
            }
            if build.runner.provenance.is_none() || build.engine.provenance.is_none() {
                return Err(Error::Configuration(format!(
                    "{} must pin runner and engine provenance",
                    build.build_id
                )));
            }
            if build.supported_operations.is_empty()
                || build.allowed_tzdb_release_kinds.is_empty()
                || build.representative_vectors.is_empty()
            {
                return Err(Error::Configuration(format!(
                    "{} has an incomplete capability/provenance/smoke declaration",
                    build.build_id
                )));
            }
            if build.reproducibility.status == ReproducibilityStatus::Unreproducible
                && build.reproducibility.reason.is_none()
            {
                return Err(Error::Configuration(format!(
                    "{} is unreproducible without a reason",
                    build.build_id
                )));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn build(&self, id: &str) -> Option<&RunnerBuild> {
        self.builds.iter().find(|build| build.build_id == id)
    }
}

/// Corpus-owned protocol schema document used to validate every inbound line
/// before typed deserialization.
#[derive(Debug, Clone)]
pub struct ProtocolSchema {
    document: Value,
}

impl ProtocolSchema {
    pub fn load(path: &Path) -> Result<Self> {
        Self::from_value(serde_json::from_slice(&fs::read(path)?)?)
    }

    pub fn from_value(document: Value) -> Result<Self> {
        jsonschema::validator_for(&document)
            .map_err(|error| Error::ProtocolSchema(error.to_string()))?;
        Ok(Self { document })
    }

    pub(crate) fn validate(&self, value: &Value) -> std::result::Result<(), String> {
        let validator =
            jsonschema::validator_for(&self.document).map_err(|error| error.to_string())?;
        validator.validate(value).map_err(|error| error.to_string())
    }
}
