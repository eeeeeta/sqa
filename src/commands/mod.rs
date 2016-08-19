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
#[derive(Copy, Clone)]
enum Commands {
    Load,
    Vol,
    Stop,
    Start,
    ReStart,
    Output
}
#[derive(Copy, Clone)]
pub struct CommandSpawner {
    cmd: Commands
}
impl CommandSpawner {
    pub fn spawn(&self) -> Box<Command> {
        match self.cmd {
            Commands::Load => Box::new(LoadCommand::new()),
            Commands::Vol => Box::new(VolCommand::new()),
            Commands::Stop => Box::new(StopStartCommand::new(stopstart::StopStartChoice::Stop)),
            Commands::Start => Box::new(StopStartCommand::new(stopstart::StopStartChoice::Start)),
            Commands::ReStart => Box::new(StopStartCommand::new(stopstart::StopStartChoice::ReStart)),
            Commands::Output => Box::new(OutputCommand::new()),
        }
    }
}
pub enum GridNode {
    Choice(CommandSpawner),
    Grid(Vec<(&'static str, gkey, GridNode)>),
    Clear,
    Execute,
    Mode,
    Go,
    GotoQ
}
pub fn get_chooser_grid() -> Vec<(&'static str, gkey, GridNode)> {
    vec![
        ("<b>Stream</b> <i>S</i>", gkeys::s, GridNode::Grid(vec![
            ("Stop <i>O</i>", gkeys::o, GridNode::Choice(CommandSpawner { cmd: Commands::Stop })),
            ("Start <i>S</i>", gkeys::s, GridNode::Choice(CommandSpawner { cmd: Commands::Start })),
            ("Restart <i>R</i>", gkeys::r, GridNode::Choice(CommandSpawner { cmd: Commands::ReStart })),
            ("Volume <i>V</i>", gkeys::v, GridNode::Choice(CommandSpawner { cmd: Commands::Vol }))
        ])),
        ("<b>I/O</b> <i>I</i>", gkeys::i, GridNode::Grid(vec![
            ("Load <i>L</i>", gkeys::l, GridNode::Choice(CommandSpawner { cmd: Commands::Load }))
        ])),
        ("<b>Mixer</b> <i>M</i>", gkeys::m, GridNode::Grid(vec![
            ("Output <i>O</i>", gkeys::o, GridNode::Choice(CommandSpawner { cmd: Commands::Output }))
        ])),
        ("<b>Cue</b> <i>Q</i>", gkeys::q, GridNode::Grid(vec![
            ("Go To <i>G</i>", gkeys::g, GridNode::GotoQ)
        ])),
        ("Mode <i>O</i>", gkeys::o, GridNode::Mode),
        ("Clear <i>C</i>", gkeys::c, GridNode::Clear),
        // The menu will overwrite the following commands' text depending on mode.
        ("[left blank]", gkeys::g, GridNode::Go),
        ("[left blank]", gkeys::Return, GridNode::Execute),
    ]
}
