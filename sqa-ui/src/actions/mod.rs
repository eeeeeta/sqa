use gtk::prelude::*;
use gtk::{TreeView, ListStore, Builder};
use uuid::Uuid;
use std::collections::HashMap;
use std::mem;
use sqa_backend::codec::Reply;
use sqa_backend::actions::{ActionParameters, OpaqueAction};
use sqa_backend::actions::audio::Controller as AudioController;

use connection::ConnectionState;

pub struct ActionController {
    view: TreeView,
    store: ListStore,
    builder: Builder,
    actions: HashMap<Uuid, Action>
}
struct Fixme;
impl ActionUI for Fixme {
    fn on_new_parameters(&mut self, p: &ActionParameters) {
    }
}
pub trait ActionUI {
    fn on_new_parameters(&mut self, p: &ActionParameters);
}
pub struct Action {
    inner: OpaqueAction,
    ctl: Box<ActionUI>
}
impl ActionController {
    pub fn new(b: &Builder) -> Self {
        let actions = HashMap::new();
        let builder = b.clone();
        build!(ActionController using b
               with actions, builder
               get view, store)
    }
    fn update_store(&mut self) {
        self.store.clear();
        for (uu, action) in self.actions.iter() {
            self.store.insert_with_values(None, &[0, 1], &[
                &uu.to_string(),
                &action.inner.desc
            ]);
        }
    }
    fn on_action_info(&mut self, uu: Uuid, data: OpaqueAction) {
        if self.actions.get_mut(&uu).is_some() {
            // FIXME(rust): the borrow checker forbids if let here, because bad desugaring.
            let act = self.actions.get_mut(&uu).unwrap();
            mem::replace(&mut act.inner, data);
            act.ctl.on_new_parameters(&act.inner.params);
        }
        else {
            let aui = Fixme;
            let mut act = Action {
                inner: data,
                ctl: Box::new(aui)
            };
            act.ctl.on_new_parameters(&act.inner.params);
            self.actions.insert(uu, act);
        }
    }
    pub fn on_action_reply(&mut self, r: Reply) {
        use self::Reply::*;
        match r {
            UpdateActionInfo { uuid, data } => self.on_action_info(uuid, data),
            UpdateActionDeleted { uuid } => {
                self.actions.remove(&uuid);
            },
            ReplyActionList { list } => {
                self.actions.clear();
                for (uu, oa) in list {
                    self.on_action_info(uu, oa);
                }
            }
            x => println!("warn: unexpected action reply {:?}", x)
        }
        self.update_store();
    }
}
