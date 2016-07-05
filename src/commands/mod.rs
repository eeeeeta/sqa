mod prelude;
mod load;
mod stopstart;
mod vol;

pub use self::load::LoadCommand;
pub use self::vol::VolCommand;
pub use self::stopstart::StopStartCommand;
use self::prelude::Command;
use gdk::enums::key::Key as gkey;
use gdk::enums::key as gkeys;
#[derive(Copy, Clone)]
enum Commands {
    Load,
    Vol,
    Stop,
    Start
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
        }
    }
}
pub enum GridNode {
    Choice(CommandSpawner),
    Grid(Vec<(&'static str, gkey, GridNode)>),
    Clear,
    Execute(bool)
}
pub fn get_chooser_grid() -> Vec<(&'static str, gkey, GridNode)> {
    vec![
        ("<b>Stream</b> <i>S</i>", gkeys::s, GridNode::Grid(vec![
            ("Stop <i>O</i>", gkeys::o, GridNode::Choice(CommandSpawner { cmd: Commands::Stop })),
            ("Start <i>S</i>", gkeys::s, GridNode::Choice(CommandSpawner { cmd: Commands::Start })),
            ("Volume <i>V</i>", gkeys::v, GridNode::Choice(CommandSpawner { cmd: Commands::Vol }))
        ])),
        ("<b>I/O</b> <i>I</i>", gkeys::i, GridNode::Grid(vec![
            ("Load <i>L</i>", gkeys::l, GridNode::Choice(CommandSpawner { cmd: Commands::Load }))
        ])),
        ("Clear <i>C</i>", gkeys::c, GridNode::Clear),
        ("Execute <b>↵</b>", gkeys::Return, GridNode::Execute(false)),
        ("ExKeep <i>X</i>", gkeys::x, GridNode::Execute(true)),
    ]
}
