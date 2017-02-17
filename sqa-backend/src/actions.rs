use futures::Future;
use std::error::Error;
use std::path::PathBuf;
use std::collections::HashMap;
use std::any::Any;
pub enum Value {
    Volume(f32),
    Path(PathBuf),
    String(String),
    U32(u32),
    I32(i32)
}
pub enum ActionError {
    NowUnloaded(Box<Error>, UnloadedAction),
    NowLoaded(Box<Error>, LoadedAction),
}

pub type ParameterMap = HashMap<&'static str, Parameter>;
pub type LoaderFuture = Box<Future<Item=LoadedAction, Error=Box<Error>>>;
pub type ExecuterFuture = Box<Future<Item=ExecutingAction, Error=ActionError>>;
pub struct Parameter {
    val: Value,
    desc: &'static str,
    name: &'static str
}
pub struct ParameterError {
    name: &'static str,
    err: String
}
pub trait ActionController {
    fn verify(&self, ula: &UnloadedAction) -> Vec<ParameterError>;
    fn load(&self, ula: UnloadedAction) -> LoaderFuture;
    fn execute(&self, la: LoadedAction) -> ExecuterFuture;
}
pub struct UnloadedAction {
    params: ParameterMap
}
pub struct LoadedAction {
    params: ParameterMap,
    magic: Box<Any>
}
pub struct ExecutingAction {
    params: ParameterMap,
    magic: Box<Any>,
}
pub enum Action {
    Unloaded(UnloadedAction, Box<ActionController>),
    Loaded(LoadedAction, Box<ActionController>),
    Executing(ExecutingAction, Box<ActionController>)
}
