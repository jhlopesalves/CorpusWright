//! Defines project IPC contracts and the persisted source-inventory domain.
//!
//! Project source IDs, manifest versions, and ordering are stable storage contracts shared by
//! import, preparation, preview, and analysis orchestration.

pub mod sources;

use crate::text::language::ProjectLanguage;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateCorpusProjectRequest {
    pub root_path: String,
    pub name: String,
    pub language: ProjectLanguage,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ActiveProject {
    pub name: String,
    pub root_path: String,
    pub manifest_path: String,
    pub project_version: u32,
    pub source_roots: Vec<String>,
    pub language: ProjectLanguage,
    pub description: Option<String>,
}
