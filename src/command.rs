use parser::{Tokens, EtokenFSM, ParserErr, SpaceRet};
use state::{ReadableContext, WritableContext, ObjectType, Database};
use rsndfile::SndFile;
use streamv2::{FileStream, FileStreamX, LiveParameters, db_lin};
use mixer::FRAMES_PER_CALLBACK;
use std::string::ToString;
use uuid::Uuid;
use std::any::Any;
use mopa;

use std::rc::Rc;
use std::cell::RefCell;

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
pub struct TextHunk {
    text: String
}
impl TextHunk {
    fn new(s: String) -> Box<Hunk> {
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
    ty: HunkTypes,
    get: Box<Fn(&U) -> Option<T>>,
    set: Box<Fn(&mut U, &ReadableContext, Option<&T>) -> Result<(), String>>
}
impl<T, U> GenericHunk<T, U> where T: Any, U: Command {
    fn new(ty: HunkTypes, get: Box<Fn(&U) -> Option<T>>, set: Box<Fn(&mut U, &ReadableContext, Option<&T>) -> Result<(), String>>) -> Box<Hunk> {
        Box::new(GenericHunk {
            ty: ty,
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
    fn get_val(&self) -> Option<Box<Any>> {
        let cmd = self.command.as_ref().unwrap().borrow();
        let cmd = cmd.downcast_ref::<U>().unwrap();
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

pub struct LoadCommand {
    file: Option<String>,
    ident: Option<String>
}
impl LoadCommand {
    pub fn new() -> Self {
        LoadCommand {
            file: None,
            ident: None
        }
    }
}
impl Command for LoadCommand {
    fn get_hunks(&self) -> Vec<Box<Hunk>> {
        let file_getter = move |selfish: &Self| -> Option<String> {
            selfish.file.as_ref().map(|x| x.clone())
        };
        let file_setter = move |selfish: &mut Self, _: &ReadableContext, val: Option<&String>| {
            if let Some(val) = val {
                let file = SndFile::open(val);
                if let Err(e) = file {
                    Err(format!("Failed to open file: {}", e.expl))
                }
                else if file.as_ref().unwrap().info.samplerate != 44_100 {
                    Err(format!("SQA only supports files with a samplerate of 44.1kHz."))
                }
                else {
                    selfish.file = Some(val.clone());
                    Ok(())
                }
            }
            else {
                selfish.file = None;
                Ok(())
            }
        };
        let ident_getter = move |selfish: &Self| -> Option<String> {
            selfish.ident.as_ref().map(|x| x.clone())
        };
        let ident_setter = move |selfish: &mut Self, ctx: &ReadableContext, val: Option<&String>| {
            if let Some(val) = val {
                if ctx.db.resolve_ident(val).is_some() {
                    Err(format!("Identifier is already in use."))
                }
                else {
                    selfish.ident = Some(val.clone());
                    Ok(())
                }
            }
            else {
                selfish.ident = None;
                Ok(())
            }
        };
        vec![
            GenericHunk::new(HunkTypes::FilePath, Box::new(file_getter), Box::new(file_setter)),
            TextHunk::new(format!("As")),
            GenericHunk::new(HunkTypes::String, Box::new(ident_getter), Box::new(ident_setter))
        ]
    }
    fn get_state(&self, ctx: &ReadableContext) -> CommandState {
        unimplemented!()
    }
    fn execute(&mut self, ctx: &mut WritableContext) -> Result<(), String> {
        unimplemented!()
    }
}
pub struct CommandState {
    title: String,
    desc: String,
    exec: bool,
    complete: bool
}
pub trait Command: mopa::Any + Send + 'static {
    fn get_hunks(&self) -> Vec<Box<Hunk>>;
    fn get_state(&self, ctx: &ReadableContext) -> CommandState;
    fn execute(&mut self, ctx: &mut WritableContext) -> Result<(), String>;
}

mopafy!(Command);
