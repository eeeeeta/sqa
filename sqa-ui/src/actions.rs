use gtk::prelude::*;
use gtk::{ListBox};
use uuid::Uuid;
use std::collections::HashMap;

pub struct ActionController {
    list: ListBox,
    actions: HashMap<Uuid, ActionType>
}
pub enum ActionType {
    Audio
}
