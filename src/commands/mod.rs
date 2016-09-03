//! Organisation and collation of commands.
#[macro_use]
mod prelude;
mod load;
mod stopstart;
mod vol;
mod output;

pub use self::load::LoadCommand;
pub use self::vol::VolCommand;
pub use self::stopstart::StopStartCommand;
pub use self::output::OutputCommand;
use self::prelude::Command;
use gdk::enums::key::Key as gkey;
use gdk::enums::key as gkeys;
/// List of possible command types for a `CommandSpawner`.
#[derive(Copy, Clone)]
enum Commands {
    Load,
    Vol,
    StopStart(stopstart::StopStartChoice),
    Output
}
/// An object that creates a command based on a `Commands` enum.
#[derive(Copy, Clone)]
pub struct CommandSpawner {
    cmd: Commands
}
impl CommandSpawner {
    pub fn spawn(&self) -> Box<Command> {
        match self.cmd {
            Commands::Load => Box::new(LoadCommand::new()),
            Commands::Vol => Box::new(VolCommand::new()),
            Commands::StopStart(c) => Box::new(StopStartCommand::new(c)),
            Commands::Output => Box::new(OutputCommand::new()),
        }
    }
}
/// A node on the grid displayed by the `CommandChooserController`.
pub enum GridNode {
    /// A command choice.
    Choice(CommandSpawner),
    /// A submenu.
    Grid(Vec<(&'static str, &'static str, gkey, GridNode)>),
    /// Clears the command line of commands.
    Clear,
    /// Toggles the "fallthru" state on the current command.
    Fallthru,
    /// Executes or saves the current command.
    Execute,
    /// Moves the current command.
    Reorder,
    /// Toggles Live/Blind state.
    Mode,
}
/// Returns the grid used by the `CommandChooserController`.
pub fn get_chooser_grid() -> Vec<(&'static str, &'static str, gkey, GridNode)> {
    vec![
        ("<b>Stream</b> <i>S</i>", "Menu for stream-related actions", gkeys::s, GridNode::Grid(vec![
            ("<b>Load</b> <i>L</i>", "Creates a stream from a file", gkeys::l, GridNode::Choice(CommandSpawner { cmd: Commands::Load })),
            ("Stop <i>O</i>", "Stops a stream", gkeys::o, GridNode::Choice(CommandSpawner { cmd: Commands::StopStart(stopstart::StopStartChoice::Stop) })),
            ("Unpause <i>U</i>", "Unpauses a stream that has been paused", gkeys::u, GridNode::Choice(CommandSpawner { cmd: Commands::StopStart(stopstart::StopStartChoice::Unpause) })),
            ("Pause <i>P</i>", "Pauses a stream", gkeys::p, GridNode::Choice(CommandSpawner { cmd: Commands::StopStart(stopstart::StopStartChoice::Pause) })),
            ("Restart <i>R</i>", "Starts playing a stream from the beginning", gkeys::r, GridNode::Choice(CommandSpawner { cmd: Commands::StopStart(stopstart::StopStartChoice::ReStart) })),
            ("Volume <i>V</i>", "Sets or fades a stream's volume", gkeys::v, GridNode::Choice(CommandSpawner { cmd: Commands::Vol }))
        ])),
        ("<b>Mixer</b> <i>M</i>", "Menu for mixer-related actions", gkeys::m, GridNode::Grid(vec![
            ("Output <i>O</i>", "Initialise the default sound output", gkeys::o, GridNode::Choice(CommandSpawner { cmd: Commands::Output }))
        ])),
        // Some of these commands have their text overwritten by the CommandChooserController
        // at runtime. It should be clear which ones they are.
        ("my hands are typing words", "", gkeys::o, GridNode::Mode),
        ("Clear <i>C</i>", "Clear the command currently on the command line", gkeys::c, GridNode::Clear),
        ("F'thru <i>F</i>", "Toggle <i>fallthrough</i> state: whether the cue runner will immediately run the command after this one, or wait for this one to finish", gkeys::f, GridNode::Fallthru),
        ("Reorder <i>R</i>", "Reposition this command, attaching it to a different cue or changing its position on a cue", gkeys::r, GridNode::Reorder),
        ("here have code", "", gkeys::Return, GridNode::Execute),
    ]
}
