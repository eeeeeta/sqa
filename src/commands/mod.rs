mod prelude;
mod load;

pub use self::load::LoadCommand;
use self::prelude::Command;

#[derive(Copy, Clone)]
enum Commands {
    Load
}
#[derive(Copy, Clone)]
pub struct CommandSpawner {
    cmd: Commands
}
impl CommandSpawner {
    pub fn spawn(&self) -> Box<Command> {
        match self.cmd {
            Commands::Load => Box::new(LoadCommand::new())
        }
    }
}
pub enum GridNode {
    Choice(CommandSpawner),
    Grid(Vec<(&'static str, GridNode)>)
}
pub fn get_chooser_grid() -> Vec<(&'static str, GridNode)> {
    vec![
        ("Stream", GridNode::Grid(vec![])),
        ("I/O", GridNode::Grid(vec![
            ("Load", GridNode::Choice(CommandSpawner { cmd: Commands::Load }))
        ])),
        ("System", GridNode::Grid(vec![])),
        ("Lorem", GridNode::Grid(vec![])),
        ("Ipsum", GridNode::Grid(vec![])),
        ("Dolor", GridNode::Grid(vec![])),
        ("Lorem", GridNode::Grid(vec![])),
        ("Ipsum", GridNode::Grid(vec![])),
        ("Dolor", GridNode::Grid(vec![]))
    ]
}
