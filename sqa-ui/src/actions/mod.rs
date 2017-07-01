use gtk::prelude::*;
use gtk::{self, Widget, Menu, TreeView, ListStore, Builder, MenuItem, TreeSelection, TargetEntry, TargetFlags, Stack};
use gdk::WindowExt;
use connection::UndoableChange;
use gdk;
use uuid::Uuid;
use std::collections::HashMap;
use std::mem;
use sync::UISender;
use sqa_backend::codec::{Command, Reply};
use sqa_backend::mixer::MixerConf;
use sqa_backend::actions::{ActionParameters, PlaybackState, OpaqueAction};
use sqa_backend::actions::audio::AudioParams;
use messages::Message;

pub mod audio;
pub mod fade;
pub mod template;
pub use self::template::UITemplate;
use self::audio::AudioUI;
use self::fade::FadeUI;
pub enum ActionInternalMessage {
    Create(&'static str),
    SelectionChanged,
    FilesDropped(Vec<String>),
    BeginSelection(Uuid),
    CancelSelection,
    ChangeCurPage(Option<u32>)
}
#[derive(Clone)]
pub enum ActionMessageInner {
    Audio(audio::AudioMessage),
    Fade(fade::FadeMessage),
    LoadAction,
    ExecuteAction,
    DeleteAction,
    ResetAction,
    EditAction,
    ChangeName(Option<String>),
    CloseButton,
}
pub type ActionMessage = (Uuid, ActionMessageInner);
struct SelectionDetails {
    initiator: Uuid,
    prev: Option<Uuid>,
    cursor: Option<gdk::Cursor>
}
pub struct ActionController {
    view: TreeView,
    mixer: MixerConf,
    store: ListStore,
    tx: Option<UISender>,
    cur_widget: Option<(Uuid, Widget)>,
    sidebar: Stack,
    ctls: HashMap<Uuid, Box<ActionUI>>,
    opas: HashMap<Uuid, OpaqueAction>,
    cur_sel: Option<SelectionDetails>,
    menu: Menu,
    medit: MenuItem,
    mdelete: MenuItem,
    mload: MenuItem,
    mexec: MenuItem,
    mcreate_audio: MenuItem,
    mcreate_fade: MenuItem,
    cur_page: Option<u32>
}
pub trait ActionUI {
    fn on_update(&mut self, p: &OpaqueAction);
    fn on_message(&mut self, m: ActionMessageInner);
    fn edit_separately(&mut self);
    fn get_container(&mut self) -> Option<Widget>;
    fn on_mixer(&mut self, _m: &MixerConf) {}
    fn close_window(&mut self) {}
    fn on_action_list(&mut self, _l: &HashMap<Uuid, OpaqueAction>) {}
    fn on_selection_finished(&mut self, _sel: Uuid) {}
    fn on_selection_cancelled(&mut self) {}
    fn change_cur_page(&mut self, Option<u32>);
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
macro_rules! bind_action_menu_items {
    ($self:ident, $tx:ident, $tsg:ident, $($name:ident => $res:ident),*) => {
        $(
            $self.$name.connect_activate(clone!($tx, $tsg; |_| {
                if let Some(uu) = $tsg.get() {
                    $tx.send_internal((uu, $res));
                }
            }));
        )*
    }
}
macro_rules! action_reply_notify {
    ($self:ident, $res:ident, $failmsg:expr, $successmsg:expr) => {
        let msg;
        if let Err(e) = $res {
            msg = Message::Error(format!(concat!($failmsg, " failed: {}"), e));
        }
        else {
            msg = Message::Statusbar($successmsg.into());
        }
        $self.tx.as_mut().unwrap().send_internal(msg);
    }
}
impl ActionController {
    pub fn new(b: &Builder) -> Self {
        let ctls = HashMap::new();
        let opas = HashMap::new();
        let tx = None;
        let cur_page = None;
        let cur_widget = None;
        let cur_sel = None;
        let mixer = Default::default();
        build!(ActionController using b
               with ctls, opas, tx, mixer, cur_widget, cur_sel, cur_page
               get view, store, menu, medit, mload, mexec, mdelete, mcreate_audio, mcreate_fade, sidebar)
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
        let menu = self.menu.clone();
        self.view.connect_button_press_event(move |_, eb| {
            if eb.get_button() == 3 {
                if let ::gdk::EventType::ButtonPress = eb.get_event_type() {
                    menu.popup_easy(eb.get_button(), eb.get_time());
                }
            }
            Inhibit(false)
        });
        self.view.get_selection().connect_changed(clone!(tx; |_| {
            tx.send_internal(ActionInternalMessage::SelectionChanged);
        }));
        self.mcreate_audio.connect_activate(clone!(tx; |_| {
            tx.send_internal(ActionInternalMessage::Create("audio"));
        }));
        self.mcreate_fade.connect_activate(clone!(tx; |_| {
            tx.send_internal(ActionInternalMessage::Create("fade"));
        }));
        let dnd_targets = vec![TargetEntry::new("text/uri-list", TargetFlags::empty(), 0)];
        self.view.drag_dest_set(gtk::DEST_DEFAULT_ALL, &dnd_targets, gdk::ACTION_COPY | gdk::ACTION_MOVE);

        self.view.connect_drag_data_received(clone!(tx; |_, _, _, _, data, _, _| {
            let uris = data.get_uris();
            debug!("dnd: got uris {:?}", uris);
            if uris.len() == 0 { return; }
            tx.send_internal(ActionInternalMessage::FilesDropped(uris));
        }));

        tx.send_internal(ActionInternalMessage::SelectionChanged);
    }
    fn update_store(&mut self, sel: Option<Uuid>) {
        let tsg = TreeSelectGetter { ts: self.store.clone(), sel: self.view.get_selection() };
        let sel = if sel.is_some() { sel } else { tsg.get() };
        self.store.clear();
        for (uu, action) in self.opas.iter() {
            let typ = match action.params {
                ActionParameters::Audio(_) => "audio-x-generic",
                ActionParameters::Fade(_) => "audio-volume-medium-symbolic"
            };
            use self::PlaybackState::*;
            let state = match action.state {
                Inactive => "",
                Unverified(_) => "gtk-dialog-warning",
                Loaded => "gtk-home",
                Loading => "gtk-refresh",
                Paused => "gtk-media-pause",
                Active(_) => "gtk-media-play",
                Errored(_) => "gtk-dialog-error"
            };
            let iter = self.store.insert_with_values(None, &[
                0, // uuid
                1, // description
                2, // icon-state (playback state icon)
                3, // icon-type (action type icon)
            ], &[
                &uu.to_string(),
                &action.display_name(),
                &state,
                &typ
            ]);
            if let Some(u2) = sel {
                if *uu == u2 {
                    tsg.sel.select_iter(&iter);
                }
            }
        }
    }
    fn on_action_info(&mut self, uu: Uuid, data: OpaqueAction) {
        if self.opas.get_mut(&uu).is_some() {
            // FIXME(rust): the borrow checker forbids if let here, because bad desugaring.
            let opa = self.opas.get_mut(&uu).unwrap();
            let ctl = self.ctls.get_mut(&uu).unwrap();
            mem::replace(opa, data);
            ctl.on_update(&opa);
        }
        else {
            let mut aui = match data.params {
                ActionParameters::Audio(..) =>
                    Box::new(AudioUI::new(data.uu, self.tx.as_ref().unwrap().clone())) as Box<ActionUI>,
                ActionParameters::Fade(..) =>
                    Box::new(FadeUI::new(data.uu, self.tx.as_ref().unwrap().clone())) as Box<ActionUI>
            };
            aui.on_update(&data);
            aui.on_mixer(&self.mixer);
            self.ctls.insert(uu, aui);
            self.opas.insert(uu, data);
        }
    }
    pub fn on_action_reply(&mut self, r: Reply) {
        use self::Reply::*;
        match r {
            UpdateActionInfo { uuid, data } => self.on_action_info(uuid, data),
            UpdateActionDeleted { uuid } => {
                self.opas.remove(&uuid);
                self.ctls.remove(&uuid);
                self.tx.as_mut().unwrap()
                    .send_internal(Message::Statusbar("Action deleted.".into()));
            },
            ReplyActionList { list } => {
                self.opas.clear();
                self.ctls.clear();
                for (uu, oa) in list {
                    self.on_action_info(uu, oa);
                }
            },
            ActionCreated { res } => {
                action_reply_notify!(self, res, "Creating action", "Action created.");
            },
            ActionLoaded { res, .. } => {
                action_reply_notify!(self, res, "Loading action", "Action loaded.");
            },
            ActionParamsUpdated { res, .. } => {
                if let Err(e) = res {
                    let msg = Message::Error(format!("Modifying action failed: {}", e));
                    self.tx.as_mut().unwrap().send_internal(msg);
                }
            },
            ActionExecuted { res, .. } => {
                action_reply_notify!(self, res, "Executing action", "Action executed.");
            },
            x => warn!("actions: unexpected action reply {:?}", x)
        }
        for (_, ctl) in self.ctls.iter_mut() {
            ctl.on_action_list(&self.opas);
        }
        self.update_store(None);
    }
    pub fn on_internal(&mut self, msg: ActionInternalMessage) {
        use self::ActionInternalMessage::*;
        match msg {
            Create(typ) => {
                let uuid = Uuid::new_v4();
                let msg = UndoableChange {
                    undo: Command::DeleteAction { uuid },
                    redo: Command::CreateActionWithUuid { typ: typ.into(), uuid },
                    desc: format!("create {} action", typ)
                };
                self.tx.as_mut().unwrap().send(msg);
            },
            SelectionChanged => {
                let tsg = TreeSelectGetter { ts: self.store.clone(), sel: self.view.get_selection() };
                if let Some(c) = tsg.get() {
                    if let Some(sel) = self.cur_sel.take() {
                        let pwin = self.view.get_parent_window().unwrap();
                        pwin.set_cursor(sel.cursor.as_ref());
                        self.update_store(sel.prev);
                        if let Some(act) = self.ctls.get_mut(&sel.initiator) {
                            act.on_selection_finished(c);
                        }
                    }
                }
                let activated = tsg.get().is_some();
                self.medit.set_sensitive(activated);
                self.mload.set_sensitive(activated);
                self.mexec.set_sensitive(activated);
                self.mdelete.set_sensitive(activated);
                if let Some(uu) = tsg.get() {
                    if let Some((u2, w)) = self.cur_widget.take() {
                        if uu == u2 {
                            self.cur_widget = Some((u2, w));
                            return;
                        }
                        self.sidebar.remove(&w);
                    }
                    if let Some(act) = self.ctls.get_mut(&uu) {
                        if let Some(w) = act.get_container() {
                            self.sidebar.add_named(&w, "action");
                            w.show_all();
                            self.sidebar.set_visible_child(&w);
                            self.cur_widget = Some((uu, w));
                        }
                        act.change_cur_page(self.cur_page);
                    }
                }
                else {
                    if let Some((_, w)) = self.cur_widget.take() {
                        self.sidebar.remove(&w);
                    }
                }
            },
            FilesDropped(files) => {
                for file in files {
                    let mut params: AudioParams = Default::default();
                    params.url = Some(file.clone().into());
                    let params = ActionParameters::Audio(params);
                    let uuid = Uuid::new_v4();
                    let msg = UndoableChange {
                        undo: Command::DeleteAction { uuid },
                        redo: Command::CreateActionWithExtras { typ: "audio".into(), params: params, uuid },
                        desc: format!("drop file {}", file)
                    };
                    self.tx.as_mut().unwrap().send(msg);
                }
            },
            BeginSelection(uu) => {
                let pwin = self.view.get_parent_window().unwrap();
                if let Some(sel) = self.cur_sel.take() {
                    pwin.set_cursor(sel.cursor.as_ref());
                    warn!("BeginSelection called twice, old initiator {} new {}", sel.initiator, uu);
                }
                let tsg = TreeSelectGetter { ts: self.store.clone(), sel: self.view.get_selection() };
                let disp = self.view.get_display().unwrap();
                let xhair = gdk::Cursor::new_from_name(&disp, "crosshair");
                let old_c = pwin.get_cursor();
                self.cur_sel = Some(SelectionDetails {
                    initiator: uu,
                    prev: tsg.get(),
                    cursor: old_c
                });
                pwin.set_cursor(&xhair);
            },
            CancelSelection => {
                if let Some(sel) = self.cur_sel.take() {
                    let pwin = self.view.get_parent_window().unwrap();
                    pwin.set_cursor(sel.cursor.as_ref());
                    self.update_store(sel.prev);
                    if let Some(act) = self.ctls.get_mut(&sel.initiator) {
                        act.on_selection_cancelled();
                    }
                }
            },
            ChangeCurPage(cp) => {
                self.cur_page = cp;
            }
        }
    }
    pub fn on_action_msg(&mut self, msg: ActionMessage) {
        let tx = self.tx.as_mut().unwrap();
        if let Some(ctl) = self.ctls.get_mut(&msg.0) {
            use self::ActionMessageInner::*;
            match msg.1 {
                LoadAction => tx.send(Command::LoadAction { uuid: msg.0 }),
                ExecuteAction => tx.send(Command::ExecuteAction { uuid: msg.0 }),
                ResetAction => tx.send(Command::ResetAction { uuid: msg.0 }),
                DeleteAction => {
                    let opa = self.opas.get_mut(&msg.0).unwrap();
                    tx.send(UndoableChange {
                        undo: Command::ReviveAction {
                            uuid: msg.0,
                            typ: opa.typ().into(),
                            params: opa.params.clone(),
                            meta: opa.meta.clone(),
                        },
                        redo: Command::DeleteAction { uuid: msg.0 },
                        desc: "delete action".into()
                    });
                },
                EditAction => {
                    if let Some((uu, w)) = self.cur_widget.take() {
                        if uu == msg.0 {
                            self.sidebar.remove(&w);
                        }
                        else {
                            self.cur_widget = Some((uu, w));
                        }
                    }
                    ctl.edit_separately()
                },
                ChangeName(name) => {
                    if let Some(opa) = self.opas.get(&msg.0) {
                        let mut meta = opa.meta.clone();
                        meta.name = name;
                        tx.send(UndoableChange {
                            undo: Command::UpdateActionMetadata { uuid: msg.0, meta: opa.meta.clone() },
                            redo: Command::UpdateActionMetadata { uuid: msg.0, meta: meta },
                            desc: "change action name".into()
                        });
                    }
                },
                CloseButton => ctl.close_window(),
                x => ctl.on_message(x)
            }
        }
    }
    pub fn on_mixer(&mut self, conf: MixerConf) {
        for (_, ctl) in self.ctls.iter_mut() {
            ctl.on_mixer(&conf);
        }
        self.mixer = conf;
    }
}
