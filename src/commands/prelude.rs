pub use command::{Command, HunkTypes, Hunk, TextHunk, GenericHunk, StreamController, new_update};
pub use state::{Context, CommandState};
pub use mio::EventLoop;
pub use uuid::Uuid;
pub use gdk::enums::key as Keys;
macro_rules! desc {
    ($x:expr) => {
        $x.as_ref().unwrap_or(&"[???]".to_owned())
    }
}
macro_rules! desc_uuid {
    ($x:expr, $ctx:expr) => {
        match $x {
            Some(ref uu) => $ctx.prettify_uuid(uu),
            None => "[???]".to_owned()
        }
    }
}
