// Agent 入口：按职责装配实现文件，保留原有调用路径。
use crate::{command_proposal::{self, PendingCommand}, context::{self, ContextPreview, PreparedContext, SourceChunk}, deep_video, notes::NotesState, read_tools::ReadIndex, storage::{AppState, KnowledgeRecord}, workflow};
use serde::{Deserialize, Serialize};
use std::{collections::{HashMap, HashSet}, fs, io::Write, path::PathBuf, sync::Mutex, time::{Duration, Instant}};
use tauri::{AppHandle, Emitter, Manager, State};
use url::Url;
use uuid::Uuid;

include!("types.rs");
include!("state.rs");
include!("chat.rs");
mod evidence;
use evidence::*;
#[cfg(test)]
pub(crate) use evidence::validate_citation_numbers;
include!("stream.rs");
include!("video_choice.rs");
include!("proposals.rs");
include!("explore.rs");
include!("suggestions.rs");
include!("compact.rs");
include!("ask.rs");

#[cfg(test)]
mod tests {
    include!("tests_first.rs");
    include!("tests_second.rs");
}
