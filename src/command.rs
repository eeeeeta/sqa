use state::{Context, CommandState};
use std::any::Any;
use mopa;
use mio::EventLoop;
use uuid::Uuid;

pub type CommandUpdate = Box<Fn(&mut Command) + Send>;
pub fn new_update<T, U>(cls: U) -> CommandUpdate where U: Fn(&mut T) + Send + 'static, T: Command {
    Box::new(move |cmd: &mut Command| {
        let cmd = cmd.downcast_mut::<T>().unwrap();
        cls(cmd);
    })
}
pub trait BoxClone {
    fn box_clone(&self) -> Box<Command>;
}
impl<T> BoxClone for T where T: Clone + Command + 'static {
    fn box_clone(&self) -> Box<Command> {
        Box::new(self.clone())
    }
}
pub trait StreamController {
    fn unpause(&mut self);
    fn pause(&mut self);
    fn restart(&mut self);
}
pub struct CommandArgs<'a> {
    ctx: &'a mut Context<'a>,
    evl: &'a mut EventLoop<Context<'a>>,
    uu: Uuid
}
/// Command thingy.
pub trait Command: mopa::Any + Send + BoxClone + 'static {
    fn name(&self) -> &'static str;
    fn desc(&self, ctx: &Context) -> String {
        format!("{}", self.name())
    }

    fn get_hunks(&self) -> Vec<Box<Hunk>>;

    fn run_state(&self) -> Option<CommandState> { None }

    fn load(&mut self, ctx: &mut Context, evl: &mut EventLoop<Context>, uu: Uuid) {}
    fn unload(&mut self, ctx: &mut Context, evl: &mut EventLoop<Context>, uu: Uuid) {}

    fn execute(&mut self, ctx: &mut Context, evl: &mut EventLoop<Context>, uu: Uuid) -> Result<bool, String>;

    fn sources(&self) -> Vec<Uuid> { vec![] }
    fn sinks(&self) -> Vec<Uuid> { vec![] }

    fn can_ctl_stream(&self) -> bool { false }
    fn ctl_stream<'a>(&'a mut self) -> Option<Box<StreamController + 'a>> { None }
    fn drop(&mut self, ctx: &mut Context, evl: &mut EventLoop<Context>, uu: Uuid) {}
}

mopafy!(Command);

#[derive(Clone, Debug)]
pub enum HunkTypes {
    /// File path: `String`
    FilePath(Option<String>),
    /// Identifier: `Uuid`
    Identifier(Option<Uuid>),
    /// Volume: `f32`
    Volume(f32),
    /// Time: `u64`
    Time(Option<u64>),
    /// Generic string: `String`
    String(Option<String>),
    /// Immutable text: `String` (setter always returns error)
    Label(String)
}
impl HunkTypes {
    pub fn is_none(&self) -> bool {
        match self {
            &HunkTypes::FilePath(ref opt) => opt.is_none(),
            &HunkTypes::Identifier(ref opt) => opt.is_none(),
            &HunkTypes::String(ref opt) => opt.is_none(),
            &HunkTypes::Label(..) => false,
            &HunkTypes::Volume(..) => false,
            &HunkTypes::Time(ref opt) => opt.is_none()
        }
    }
    pub fn unwrap_ref(&self) -> &Any {
        match self {
            &HunkTypes::FilePath(ref opt) => opt,
            &HunkTypes::Identifier(ref opt) => opt,
            &HunkTypes::String(ref opt) => opt,
            &HunkTypes::Label(ref opt) => opt,
            &HunkTypes::Volume(ref opt) => opt,
            &HunkTypes::Time(ref opt) => opt,
        }
    }
    pub fn none_of(&self) -> HunkTypes {
        match self {
            &HunkTypes::FilePath(..) => HunkTypes::FilePath(None),
            &HunkTypes::Identifier(..) => HunkTypes::Identifier(None),
            &HunkTypes::String(..) => HunkTypes::String(None),
            &HunkTypes::Label(..) => panic!("eta dun goofed"),
            &HunkTypes::Volume(..) => HunkTypes::Volume(0.0),
            &HunkTypes::Time(..) => HunkTypes::Time(None)
        }
    }
    pub fn string_of(&self, st: Option<String>) -> HunkTypes {
        match self {
            &HunkTypes::FilePath(..) => HunkTypes::FilePath(st),
            &HunkTypes::String(..) => HunkTypes::String(st),
            _ => panic!("eta dun goofed"),
        }
    }
}
#[derive(Clone, Debug)]
pub struct HunkState {
    pub val: HunkTypes,
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
    /// Gets this hunk's value and state.
    fn get_val(&self, cmd: &Command, ctx: &Context) -> HunkState;
    /// Sets this hunk's value - dependent on what type it is.
    fn set_val(&mut self, cmd: &mut Command, val: HunkTypes);
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
    fn get_val(&self, _: &Command, _: &Context) -> HunkState {
        HunkState {
            val: HunkTypes::Label(self.text.clone()),
            required: false,
            help: "eta is a lousy coder",
            err: None
        }
    }
    fn set_val(&mut self, _: &mut Command, _: HunkTypes) {
        panic!("Hunk method called on text hunk");
    }
}

/// An implementation of the hunk API.
pub struct GenericHunk {
    pub get: Box<Fn(&Command, &Context) -> HunkState>,
    pub set: Box<Fn(&mut Command, HunkTypes)>,
}
impl Hunk for GenericHunk {
    fn get_val(&self, cmd: &Command, ctx: &Context) -> HunkState {
        let ref getter = self.get;
        getter(cmd, ctx)
    }
    fn set_val(&mut self, cmd: &mut Command, val: HunkTypes) {
        let ref setter = self.set;
        setter(cmd, val)
    }
}
macro_rules! hunk {
    ($typ:ident, $hlp:expr, $reqd:expr, $getter:expr, $setter:expr, $egetter:expr) => {{
        use state::Context;
        use command::{Command, HunkTypes, HunkState};
        let get_val = move |cmd: &Command, ctx: &Context| -> HunkState {
            let cmd = cmd.downcast_ref().unwrap();
            HunkState {
                val: HunkTypes::$typ($getter(cmd)),
                required: $reqd,
                help: $hlp,
                err: $egetter(cmd, ctx)
            }
        };
        let set_val = move |cmd: &mut Command, val: HunkTypes| {
            if let HunkTypes::$typ(v) = val {
                $setter(cmd.downcast_mut().unwrap(), v);
            }
            else {
                panic!("wrong type");
            }
        };
        Box::new(GenericHunk {
            get: Box::new(get_val),
            set: Box::new(set_val),
        })
    }}
}
