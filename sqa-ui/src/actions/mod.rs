use gtk::prelude::*;
use gtk::{TreeView, ListStore, Button, ButtonBox, ButtonBoxStyle, Orientation, Builder};
use uuid::Uuid;
use std::collections::HashMap;
use std::mem;
use sync::UISender;
use widgets::PropertyWindow;
use sqa_backend::codec::{Command, Reply};
use sqa_backend::mixer::MixerConf;
use sqa_backend::actions::{ActionParameters, PlaybackState, OpaqueAction};
use sqa_backend::actions::audio::Controller as AudioController;

use connection::ConnectionState;

pub mod audio;
use self::audio::AudioUI;
pub enum ActionMessageInner {
    Audio(audio::AudioMessage),
    LoadAction,
    ExecuteAction,
}
pub type ActionMessage = (Uuid, ActionMessageInner);
pub struct ActionController {
    view: TreeView,
    mixer: MixerConf,
    store: ListStore,
    builder: Builder,
    tx: Option<UISender>,
    actions: HashMap<Uuid, Action>
}
pub trait ActionUI {
    fn on_update(&mut self, p: &OpaqueAction);
    fn on_message(&mut self, m: ActionMessageInner);
    fn show(&mut self);
    fn on_mixer(&mut self, m: &MixerConf) {}
}
pub trait ActionUIMessage {
    fn apply() -> ActionMessageInner;
    fn ok() -> ActionMessageInner;
    fn cancel() -> ActionMessageInner;
}
pub struct UITemplate {
    pub pwin: PropertyWindow,
    pub apply_btn: Button,
    pub ok_btn: Button,
    pub cancel_btn: Button,
    pub load_btn: Button,
    pub execute_btn: Button,
    pub tx: UISender,
    pub uu: Uuid
}

impl UITemplate {
    pub fn new(uu: Uuid, tx: UISender) -> Self {
        let mut pwin = PropertyWindow::new();
        let apply_btn = Button::new_with_mnemonic("_Apply");
        let ok_btn = Button::new_with_mnemonic("_OK");
        let cancel_btn = Button::new_with_mnemonic("_Cancel");
        let load_btn = Button::new_with_mnemonic("_Load");
        let execute_btn = Button::new_with_mnemonic("_Execute");
        let btn_box = ButtonBox::new(Orientation::Horizontal);
        btn_box.set_layout(ButtonBoxStyle::Spread);
        btn_box.pack_start(&load_btn, false, false, 0);
        btn_box.pack_start(&execute_btn, false, false, 0);
        pwin.append_button(&ok_btn);
        pwin.append_button(&cancel_btn);
        pwin.append_button(&apply_btn);
        pwin.props_box.pack_start(&btn_box, false, false, 0);
        UITemplate { pwin, apply_btn, ok_btn, cancel_btn, load_btn, execute_btn, tx, uu }
    }
    pub fn bind<T: ActionUIMessage>(&mut self) {
        let uu = self.uu;
        let ref tx = self.tx;
        self.apply_btn.connect_clicked(clone!(tx; |_a| {
            tx.send_internal((uu, T::apply()));
        }));
        self.ok_btn.connect_clicked(clone!(tx; |_a| {
            tx.send_internal((uu, T::ok()));
        }));
        self.cancel_btn.connect_clicked(clone!(tx; |_a| {
            tx.send_internal((uu, T::cancel()));
        }));
        self.load_btn.connect_clicked(clone!(tx; |_a| {
            tx.send_internal((uu, ActionMessageInner::LoadAction));
        }));
        self.execute_btn.connect_clicked(clone!(tx; |_a| {
            tx.send_internal((uu, ActionMessageInner::ExecuteAction));
        }));
    }
    pub fn on_update(&mut self, p: &OpaqueAction) {
        playback_state_update(p, &mut self.pwin);
    }
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
        let mixer = Default::default();
        build!(ActionController using b
               with actions, builder, tx, mixer
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
            act.ctl.on_mixer(&self.mixer);
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
        let tx = self.tx.as_mut().unwrap();
        if let Some(ref mut act) = self.actions.get_mut(&msg.0) {
            use self::ActionMessageInner::*;
            match msg.1 {
                LoadAction => tx.send(Command::LoadAction { uuid: msg.0 }),
                ExecuteAction => tx.send(Command::ExecuteAction { uuid: msg.0 }),
                x => act.ctl.on_message(x)
            }
        }
    }
    pub fn on_mixer(&mut self, conf: MixerConf) {
        for (_, act) in self.actions.iter_mut() {
            act.ctl.on_mixer(&conf);
        }
        self.mixer = conf;
    }
}
