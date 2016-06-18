use state::{ReadableContext, WritableContext};
use std::any::Any;
use mopa;
pub use commands::*;
use std::rc::Rc;
use std::cell::RefCell;
/// State of a command.
pub struct CommandState {
    pub message: String,
    pub complete: bool
}
impl CommandState {
    pub fn good(st: String) -> Self {
        CommandState {
            message: st,
            complete: true
        }
    }
    pub fn bad(st: String) -> Self {
        CommandState {
            message: st,
            complete: false
        }
    }
}
/// Command thingy.
pub trait Command: mopa::Any + Send + 'static {
    fn get_hunks(&self) -> Vec<Box<Hunk>>;
    fn get_state(&self, ctx: &ReadableContext) -> CommandState;
    fn execute(&mut self, ctx: &mut WritableContext) -> Result<(), String>;
}

mopafy!(Command);

#[derive(Clone, Copy, Debug)]
pub enum HunkTypes {
    /// File path: `String`
    FilePath,
    /// Identifier: `String`
    Identifier,
    /// Volume: `f32`
    Volume,
    Time,
    Num,
    /// Generic string: `String`
    String,
    /// Immutable text: `String` (setter always returns error)
    Label
}
macro_rules! get_str_and {
    ($x:expr, $a:expr) => {{
        get_typ_and!($x, String => str, $a)
    }}
}
macro_rules! get_typ_and {
    ($x:expr, $t1:ty => $t2:ty, $a:expr) => {{
        match get_and_coerce!($x, $t1) {
            Some(st) => {
                let strn = Some(&*st as &$t2);
                $a(strn)
            },
            None => $a(None)
        }
    }}
}


macro_rules! get_and_coerce {
    ($x:expr, $t:ty) => {{
        let val = $x.get_val();
        match val {
            Some(bx) => {
                Some(bx.downcast::<$t>().expect("get_and_coerce!() called with incorrect type"))
            }
            None => None
        }
    }}
}
/// Describes a hunk of a command line that controls a specific parameter.
///
/// # About hunks
/// Hunks are elements that make up a command line that offer specific behaviour based on
/// their type. For example, a `FilePath` hunk might offer a popover with a file chooser dialog.
///
/// # Invariants
/// Hunks are expected to use given types for `get_val` and `set_val`, depending on their type.
/// See the `HunkTypes` enum for which ones.
pub trait Hunk {
    /// Gives this hunk a reference to its command.
    /// This must be called before attempting to do anything with the hunk.
    fn assoc(&mut self, host: Rc<RefCell<Box<Command>>>);
    fn help(&self) -> &'static str { "If you're seeing this, someone forgot to add help." }
    /// Gives the type of this hunk - what UI element it should resemble.
    fn disp(&self) -> HunkTypes;
    /// Gets this hunk's value - dependent on what type it is.
    fn get_val(&self) -> Option<Box<Any>>;
    /// Sets this hunk's value - dependent on what type it is.
    /// This may return with an error string if the value is invalid.
    ///
    /// A `None` value for `val` unsets the value.
    fn set_val(&mut self, ctx: &ReadableContext, val: Option<Box<Any>>) -> Result<(), String>;
}

/// Static hunk that displays a bit of text.
/// Intended to add wording to a command line, like `As`.
pub struct TextHunk {
    text: String
}
impl TextHunk {
    pub fn new(s: String) -> Box<Hunk> {
        Box::new(TextHunk { text: s })
    }
}
impl Hunk for TextHunk {
    fn assoc(&mut self, _: Rc<RefCell<Box<Command>>>) {}
    fn disp(&self) -> HunkTypes {
        HunkTypes::Label
    }
    fn get_val(&self) -> Option<Box<Any>> {
        Some(Box::new(self.text.clone()))
    }
    fn set_val(&mut self, _: &ReadableContext, _: Option<Box<Any>>) -> Result<(), String> {
        Err(format!("Setting a text hunk should never happen. Something's gone wrong."))
    }
}

/// An implementation of the hunk API.
pub struct GenericHunk<T, U> where T: Any, U: Command {
    /// A reference to the command this hunk is from.
    command: Option<Rc<RefCell<Box<Command>>>>,
    hlp: &'static str,
    ty: HunkTypes,
    get: Box<Fn(&U) -> Option<T>>,
    set: Box<Fn(&mut U, &ReadableContext, Option<&T>) -> Result<(), String>>
}


impl<T, U> GenericHunk<T, U> where T: Any, U: Command {
    pub fn new(ty: HunkTypes, hlp: &'static str, get: Box<Fn(&U) -> Option<T>>, set: Box<Fn(&mut U, &ReadableContext, Option<&T>) -> Result<(), String>>) -> Box<Hunk> {
        Box::new(GenericHunk {
            ty: ty,
            hlp: hlp,
            command: None,
            get: get,
            set: set
        })
    }
}
impl<T, U> Hunk for GenericHunk<T, U> where T: Any, U: Command {
    fn assoc(&mut self, host: Rc<RefCell<Box<Command>>>) {
        self.command = Some(host);
    }
    fn disp(&self) -> HunkTypes {
        self.ty
    }
    fn help(&self) -> &'static str {
        self.hlp
    }
    fn get_val(&self) -> Option<Box<Any>> {
        let cmd = self.command.as_ref().unwrap().borrow();
        let cmd = cmd.downcast_ref().unwrap();
        let getter = &self.get;
        getter(cmd).map(|x| Box::new(x) as Box<Any>)
    }
    fn set_val(&mut self, ctx: &ReadableContext, val: Option<Box<Any>>) -> Result<(), String> {
        let mut cmd = self.command.as_ref().unwrap().borrow_mut();
        let mut cmd = cmd.downcast_mut().unwrap();
        let setter = &mut self.set;
        if let Some(ty) = val {
            let new_val = Some(ty.downcast_ref().expect("GenericHunk got wrong type for set()"));
            setter(cmd, ctx, new_val)
        }
        else {
            setter(cmd, ctx, None)
        }
    }
}
