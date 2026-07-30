use serde::Serialize;

/// Every failure the command layer can produce.
///
/// The renderer is not a trusted caller (§6.8), so validation failures are a
/// first-class variant rather than a panic. Error copy follows the
/// "what failed · why · the action" pattern from §3.10; `user_message`
/// produces the *why*, and the UI supplies the action.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("serialisation error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0} not found")]
    NotFound(&'static str),

    #[error("{0}")]
    Invalid(String),

    /// A block may not cross local midnight (§2.4, §6.5).
    #[error("A block has to start and end on the same day. Shorten it, or start it earlier.")]
    CrossesMidnight,

    /// §6.5: subtask depth is capped at 3.
    #[error("Subtasks can be nested three levels deep. Move this task higher up first.")]
    SubtaskTooDeep,

    /// §4.5: no timer may start while a recovery decision is pending.
    #[error("A session from the last run is still unresolved. Resolve it before starting a timer.")]
    RecoveryPending,

    /// §6.6: refuse to open a database written by a newer build.
    #[error("This database was written by a newer version of Fruit (schema {on_disk}, this build understands {supported}). Update Fruit to open it.")]
    NewerSchema { on_disk: i64, supported: i64 },

    #[error("{0}")]
    Import(String),
}

impl AppError {
    pub fn invalid(msg: impl Into<String>) -> Self {
        AppError::Invalid(msg.into())
    }

    /// Stable machine-readable code, for the renderer to branch on without
    /// string-matching prose.
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Db(_) => "db",
            AppError::Serde(_) => "serde",
            AppError::Io(_) => "io",
            AppError::NotFound(_) => "not_found",
            AppError::Invalid(_) => "invalid",
            AppError::CrossesMidnight => "crosses_midnight",
            AppError::SubtaskTooDeep => "subtask_too_deep",
            AppError::RecoveryPending => "recovery_pending",
            AppError::NewerSchema { .. } => "newer_schema",
            AppError::Import(_) => "import",
        }
    }
}

/// Wire shape for the renderer: a code to branch on and a sentence to show.
#[derive(Debug, Serialize)]
pub struct WireError {
    pub code: String,
    pub message: String,
}

impl AppError {
    pub fn to_wire(&self) -> WireError {
        WireError {
            code: self.code().to_string(),
            message: self.to_string(),
        }
    }
}

/// Errors cross IPC as `{ code, message }` so the renderer can branch on the
/// code and show the sentence without string-matching prose.
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        self.to_wire().serialize(s)
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
