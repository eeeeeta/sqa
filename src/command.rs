use state::{ReadableContext, WritableContext};
use std::any::Any;
use mopa;

/// Command thingy.
pub trait Command: mopa::Any + Send + 'static {
    fn name(&self) -> &'static str;
    fn get_hunks(&self) -> Vec<Box<Hunk>>;
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
pub struct HunkState {
    pub val: Option<Box<Any>>,
    pub required: bool,
    pub help: &'static str,
    pub err: Option<String>
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
    fn disp(&self) -> HunkTypes;
    /// Gets this hunk's value and state.
    fn get_val(&self, cmd: &Box<Command>, ctx: &ReadableContext) -> HunkState;
    /// Sets this hunk's value - dependent on what type it is.
    fn set_val(&mut self, cmd: &mut Box<Command>, val: Option<Box<Any>>);
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
    fn disp(&self) -> HunkTypes {
        HunkTypes::Label
    }
    fn get_val(&self, _: &Box<Command>, _: &ReadableContext) -> HunkState {
        HunkState {
            val: Some(Box::new(self.text.clone())),
            required: false,
            help: "just a simple little text thing, doing its job",
            err: None
        }
    }
    fn set_val(&mut self, _: &mut Box<Command>, _: Option<Box<Any>>) {
        panic!("Hunk method called on text hunk");
    }
}

/// An implementation of the hunk API.
pub struct GenericHunk<T, U> where T: Any, U: Command {
    hlp: &'static str,
    ty: HunkTypes,
    get: Box<Fn(&U) -> Option<T>>,
    set: Box<Fn(&mut U, Option<&T>)>,
    err: Box<Fn(&U, &ReadableContext) -> Option<String>>,
    required: bool
}


impl<T, U> GenericHunk<T, U> where T: Any, U: Command {
    pub fn new(ty: HunkTypes, hlp: &'static str, reqd: bool,
               get: Box<Fn(&U) -> Option<T>>,
               set: Box<Fn(&mut U, Option<&T>)>,
               err: Box<Fn(&U, &ReadableContext) -> Option<String>>) -> Box<Hunk> {
        Box::new(GenericHunk {
            ty: ty,
            hlp: hlp,
            get: get,
            set: set,
            err: err,
            required: reqd
        })
    }
}
impl<T, U> Hunk for GenericHunk<T, U> where T: Any, U: Command {
    fn disp(&self) -> HunkTypes {
        self.ty
    }
    fn get_val(&self, cmd: &Box<Command>, ctx: &ReadableContext) -> HunkState {
        let cmd = cmd.downcast_ref().unwrap();
        let getter = &self.get;
        let err_getter = &self.err;
        HunkState {
            val: getter(cmd).map(|x| Box::new(x) as Box<Any>),
            required: self.required,
            help: self.hlp,
            err: err_getter(cmd, ctx)
        }
    }
    fn set_val(&mut self, cmd: &mut Box<Command>, val: Option<Box<Any>>) {
        let mut cmd = cmd.downcast_mut().unwrap();
        let setter = &mut self.set;
        if let Some(ref ty) = val {
            let new_val = Some(ty.downcast_ref().expect("GenericHunk got wrong type for set()"));
            setter(cmd, new_val)
        }
        else {
            setter(cmd, None)
        };
    }
}
