use gtk::prelude::*;
use gtk::{TreeView, ListStore, Builder};
use uuid::Uuid;
use std::collections::HashMap;
use std::mem;
use sync::UISender;
use widgets::PropertyWindow;
use sqa_backend::codec::Reply;
use sqa_backend::actions::{ActionParameters, PlaybackState, OpaqueAction};
use sqa_backend::actions::audio::Controller as AudioController;

use connection::ConnectionState;

pub mod audio;
use self::audio::AudioUI;
pub enum ActionMessageInner {
    Audio(audio::AudioMessage)
}
pub type ActionMessage = (Uuid, ActionMessageInner);
pub struct ActionController {
    view: TreeView,
    store: ListStore,
    builder: Builder,
    tx: Option<UISender>,
    actions: HashMap<Uuid, Action>
}
pub trait ActionUI {
    fn on_update(&mut self, p: &OpaqueAction);
    fn on_message(&mut self, m: ActionMessageInner);
    fn show(&mut self);
}
pub fn playback_state_update(p: &OpaqueAction, pwin: &mut PropertyWindow) {
    use self::PlaybackState::*;
    match p.state {
        Inactive => pwin.update_header(
            "gtk-media-stop",
            "Inactive",
            &p.desc
        ),
        Unverified(ref errs) => pwin.update_header(
            "gtk-dialog-error",
            "Incomplete",
            format!("{} errors are present.", errs.len())
        ),
        Loading => pwin.update_header(
            "gtk-refresh",
            "Loading",
            &p.desc
        ),
        Loaded => pwin.update_header(
            "gtk-home",
            "Loaded",
            &p.desc
        ),
        Paused => pwin.update_header(
            "gtk-media-pause",
            "Paused",
            &p.desc
        ),
        Active(ref dur) => pwin.update_header(
            "gtk-media-play",
            format!("Active ({}s)", dur.as_secs()),
            &p.desc
        ),
        _ => {}
    }
}
pub struct Action {
    inner: OpaqueAction,
    ctl: Box<ActionUI>
}
impl ActionController {
    pub fn new(b: &Builder) -> Self {
        let actions = HashMap::new();
        let builder = b.clone();
        let tx = None;
        build!(ActionController using b
               with actions, builder, tx
               get view, store)
    }
    pub fn bind(&mut self, tx: &UISender) {
        self.tx = Some(tx.clone());
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
            act.ctl.on_update(&act.inner);
        }
        else {
            let mut aui = match data.params {
                ActionParameters::Audio(..) =>
                    Box::new(AudioUI::new(&self.builder, data.uu, self.tx.as_ref().unwrap().clone()))
            };
            aui.show();
            let mut act = Action {
                inner: data,
                ctl: aui
            };
            act.ctl.on_update(&act.inner);
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
    pub fn on_internal(&mut self, msg: ActionMessage) {
        if let Some(ref mut act) = self.actions.get_mut(&msg.0) {
            act.ctl.on_message(msg.1);
        }
    }
}
