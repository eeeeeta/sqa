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
/// List of possible command types for a CommandSpawner.
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
    Grid(Vec<(&'static str, gkey, GridNode)>),
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
pub fn get_chooser_grid() -> Vec<(&'static str, gkey, GridNode)> {
    vec![
        ("<b>Stream</b> <i>S</i>", gkeys::s, GridNode::Grid(vec![
            ("<b>Load</b> <i>L</i>", gkeys::l, GridNode::Choice(CommandSpawner { cmd: Commands::Load })),
            ("Stop <i>O</i>", gkeys::o, GridNode::Choice(CommandSpawner { cmd: Commands::StopStart(stopstart::StopStartChoice::Stop) })),
            ("Unpause <i>U</i>", gkeys::u, GridNode::Choice(CommandSpawner { cmd: Commands::StopStart(stopstart::StopStartChoice::Unpause) })),
            ("Pause <i>P</i>", gkeys::p, GridNode::Choice(CommandSpawner { cmd: Commands::StopStart(stopstart::StopStartChoice::Pause) })),
            ("Restart <i>R</i>", gkeys::r, GridNode::Choice(CommandSpawner { cmd: Commands::StopStart(stopstart::StopStartChoice::ReStart) })),
            ("Volume <i>V</i>", gkeys::v, GridNode::Choice(CommandSpawner { cmd: Commands::Vol }))
        ])),
        ("<b>Mixer</b> <i>M</i>", gkeys::m, GridNode::Grid(vec![
            ("Output <i>O</i>", gkeys::o, GridNode::Choice(CommandSpawner { cmd: Commands::Output }))
        ])),
        // Some of these commands have their text overwritten by the CommandChooserController
        // at runtime. It should be clear which ones they are.
        ("my hands are typing words", gkeys::o, GridNode::Mode),
        ("Clear <i>C</i>", gkeys::c, GridNode::Clear),
        ("F'thru <i>F</i>", gkeys::f, GridNode::Fallthru),
        ("Reorder <i>R</i>", gkeys::r, GridNode::Reorder),
        ("here have code", gkeys::Return, GridNode::Execute),
    ]
}
