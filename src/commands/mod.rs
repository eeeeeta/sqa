mod prelude;
mod load;
mod vol;

pub use self::load::LoadCommand;
pub use self::vol::VolCommand;
use self::prelude::Command;
use gdk::enums::key::Key as gkey;
use gdk::enums::key as gkeys;
#[derive(Copy, Clone)]
enum Commands {
    Load,
    Vol
}
#[derive(Copy, Clone)]
pub struct CommandSpawner {
    cmd: Commands
}
impl CommandSpawner {
    pub fn spawn(&self) -> Box<Command> {
        match self.cmd {
            Commands::Load => Box::new(LoadCommand::new()),
            Commands::Vol => Box::new(VolCommand::new())
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
            ("Volume <i>V</i>", gkeys::v, GridNode::Choice(CommandSpawner { cmd: Commands::Vol }))
        ])),
        ("<b>I/O</b> <i>I</i>", gkeys::i, GridNode::Grid(vec![
            ("Load <i>L</i>", gkeys::l, GridNode::Choice(CommandSpawner { cmd: Commands::Load }))
        ])),
        ("Clear <i>C</i>", gkeys::c, GridNode::Clear),
        ("Execute <i>E</i>", gkeys::e, GridNode::Execute(false)),
        ("ExKeep <i>X</i>", gkeys::x, GridNode::Execute(true)),
    ]
}
