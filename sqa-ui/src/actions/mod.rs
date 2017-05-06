use gtk::prelude::*;
use gtk::{TreeView, ListStore, Button, ButtonBox, ButtonBoxStyle, Orientation, Builder, MenuItem, TreeSelection};
use uuid::Uuid;
use std::collections::HashMap;
use std::mem;
use sync::UISender;
use widgets::PropertyWindow;
use sqa_backend::codec::{Command, Reply};
use sqa_backend::mixer::MixerConf;
use sqa_backend::actions::{ActionParameters, PlaybackState, OpaqueAction};

pub mod audio;
use self::audio::AudioUI;
pub enum ActionInternalMessage {
    Create(&'static str),
    SelectionChanged
}
pub enum ActionMessageInner {
    Audio(audio::AudioMessage),
    LoadAction,
    ExecuteAction,
    DeleteAction,
    EditAction
}
pub type ActionMessage = (Uuid, ActionMessageInner);
pub struct ActionController {
    view: TreeView,
    mixer: MixerConf,
    store: ListStore,
    builder: Builder,
    tx: Option<UISender>,
    actions: HashMap<Uuid, Action>,
    medit: MenuItem,
    mdelete: MenuItem,
    mload: MenuItem,
    mexec: MenuItem,
    mcreate_audio: MenuItem
}
pub trait ActionUI {
    fn on_update(&mut self, p: &OpaqueAction);
    fn on_message(&mut self, m: ActionMessageInner);
    fn show(&mut self);
    fn on_mixer(&mut self, _m: &MixerConf) {}
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
#[derive(Clone)]
struct TreeSelectGetter {
    sel: TreeSelection,
    ts: ListStore
}
impl TreeSelectGetter {
    pub fn get(&self) -> Option<Uuid> {
        if let Some((_, ti)) = self.sel.get_selected() {
            if let Some(v) = self.ts.get_value(&ti, 0).get::<String>() {
                if let Ok(uu) = Uuid::parse_str(&v) {
                    return Some(uu);
                }
            }
        }
        None
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
macro_rules! bind_action_menu_items {
    ($self:ident, $tx:ident, $tsg:ident, $($name:ident => $res:ident),*) => {
        $(
            $self.$name.connect_activate(clone!($tx, $tsg; |_s| {
                if let Some(uu) = $tsg.get() {
                    $tx.send_internal((uu, $res));
                }
            }));
        )*
    }
}
impl ActionController {
    pub fn new(b: &Builder) -> Self {
        let actions = HashMap::new();
        let builder = b.clone();
        let tx = None;
        let mixer = Default::default();
        build!(ActionController using b
               with actions, builder, tx, mixer
               get view, store, medit, mload, mexec, mdelete, mcreate_audio)
    }
    pub fn bind(&mut self, tx: &UISender) {
        use self::ActionMessageInner::*;
        self.tx = Some(tx.clone());
        let tsg = TreeSelectGetter { ts: self.store.clone(), sel: self.view.get_selection() };
        bind_action_menu_items! {
            self, tx, tsg,
            medit => EditAction,
            mexec => ExecuteAction,
            mload => LoadAction,
            mdelete => DeleteAction
        }
        self.view.get_selection().connect_changed(clone!(tx; |_s| {
            tx.send_internal(ActionInternalMessage::SelectionChanged);
        }));
        self.mcreate_audio.connect_activate(clone!(tx; |_s| {
            tx.send_internal(ActionInternalMessage::Create("audio"));
        }));
        tx.send_internal(ActionInternalMessage::SelectionChanged);
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
            let aui = match data.params {
                ActionParameters::Audio(..) =>
                    Box::new(AudioUI::new(&self.builder, data.uu, self.tx.as_ref().unwrap().clone()))
            };
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
    pub fn on_internal(&mut self, msg: ActionInternalMessage) {
        use self::ActionInternalMessage::*;
        match msg {
            Create(typ) => {
                self.tx.as_mut().unwrap().send(Command::CreateAction { typ: typ.into() });
            },
            SelectionChanged => {
                let activated = self.view.get_selection().get_selected().is_some();
                self.medit.set_sensitive(activated);
                self.mload.set_sensitive(activated);
                self.mexec.set_sensitive(activated);
                self.mdelete.set_sensitive(activated);
            },
        }
    }
    pub fn on_action_msg(&mut self, msg: ActionMessage) {
        let tx = self.tx.as_mut().unwrap();
        if let Some(ref mut act) = self.actions.get_mut(&msg.0) {
            use self::ActionMessageInner::*;
            match msg.1 {
                LoadAction => tx.send(Command::LoadAction { uuid: msg.0 }),
                ExecuteAction => tx.send(Command::ExecuteAction { uuid: msg.0 }),
                DeleteAction => tx.send(Command::DeleteAction { uuid: msg.0 }),
                EditAction => act.ctl.show(),
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
