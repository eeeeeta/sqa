pub use command::{Command, HunkTypes, Hunk, TextHunk, GenericHunk, new_update};
pub use state::{Context, Database, CommandState};
pub use mio::EventLoop;
pub use uuid::Uuid;

macro_rules! desc {
    ($x:expr) => {
        $x.as_ref().unwrap_or(&"<span foreground=\"red\">[???]</span>".to_owned())
    }
}
