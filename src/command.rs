use state::{ReadableContext, WritableContext};
use std::any::Any;
use mopa;

/// Command thingy.
pub trait Command: mopa::Any + Send + 'static {
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
pub struct HunkState<'a> {
    pub val: Option<Box<Any>>,
    pub required: bool,
    pub help: &'static str,
    pub stored: Option<&'a Box<Any>>,
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
    /// Gets this hunk's value - dependent on what type it is.
    fn get_val(&self, cmd: &Box<Command>, ctx: &ReadableContext) -> HunkState;
    /// Sets this hunk's value - dependent on what type it is.
    /// Can't fail - if there is a problem, the hunk should store the user's value (and return it
    /// when get_val() is called)
    fn set_val(&mut self, cmd: &mut Box<Command>, ctx: &ReadableContext, val: Option<Box<Any>>);
    /// Clears this hunk's stored value.
    fn clear(&mut self);
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
            stored: None,
            err: None
        }
    }
    fn set_val(&mut self, _: &mut Box<Command>, _: &ReadableContext, _: Option<Box<Any>>) {
        panic!("Hunk method called on text hunk");
    }
    fn clear(&mut self) {}
}

/// An implementation of the hunk API.
pub struct GenericHunk<T, U> where T: Any, U: Command {
    hlp: &'static str,
    ty: HunkTypes,
    get: Box<Fn(&U) -> Option<T>>,
    set: Box<Fn(&mut U, &ReadableContext, Option<&T>) -> Result<(), String>>,
    err: Box<Fn(&U, &ReadableContext) -> Option<String>>,
    stored_val: Option<Box<Any>>,
    stored_err: Option<String>,
    required: bool
}


impl<T, U> GenericHunk<T, U> where T: Any, U: Command {
    pub fn new(ty: HunkTypes, hlp: &'static str, reqd: bool,
               get: Box<Fn(&U) -> Option<T>>,
               set: Box<Fn(&mut U, &ReadableContext, Option<&T>) -> Result<(), String>>,
               err: Box<Fn(&U, &ReadableContext) -> Option<String>>) -> Box<Hunk> {
        Box::new(GenericHunk {
            ty: ty,
            hlp: hlp,
            get: get,
            set: set,
            err: err,
            required: reqd,
            stored_val: None,
            stored_err: None
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
            stored: self.stored_val.as_ref(),
            err: if self.stored_err.is_some() {
                Some(self.stored_err.as_ref().unwrap().clone())
            }
            else {
                err_getter(cmd, ctx)
            }
        }
    }
    fn set_val(&mut self, cmd: &mut Box<Command>, ctx: &ReadableContext, val: Option<Box<Any>>) {
        let mut cmd = cmd.downcast_mut().unwrap();
        let setter = &mut self.set;
        let ret = if let Some(ref ty) = val {
            let new_val = Some(ty.downcast_ref().expect("GenericHunk got wrong type for set()"));
            setter(cmd, ctx, new_val)
        }
        else {
            setter(cmd, ctx, None)
        };
        if ret.is_err() {
            self.stored_val = val;
            self.stored_err = ret.err();
        }
    }
    fn clear(&mut self) {
        self.stored_val = None;
        self.stored_err = None;
    }
}
