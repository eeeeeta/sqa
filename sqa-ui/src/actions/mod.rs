use gtk::prelude::*;
use gtk::{self, Widget, TreeView, ListStore, Button, ButtonBox, ButtonBoxStyle, Orientation, Builder, MenuItem, TreeSelection, TargetEntry, TargetFlags, Stack, ScrolledWindow};
use gdk;
use uuid::Uuid;
use std::collections::HashMap;
use std::mem;
use sync::UISender;
use widgets::PropertyWindow;
use sqa_backend::codec::{Command, Reply};
use sqa_backend::mixer::MixerConf;
use sqa_backend::actions::{ActionParameters, PlaybackState, OpaqueAction};
use sqa_backend::actions::audio::AudioParams;
use messages::Message;

pub mod audio;
pub mod fade;
use self::audio::AudioUI;
use self::fade::FadeUI;
pub enum ActionInternalMessage {
    Create(&'static str),
    SelectionChanged,
    FilesDropped(Vec<String>),
    BeginSelection(Uuid),
    CancelSelection
}
#[derive(Clone)]
pub enum ActionMessageInner {
    Audio(audio::AudioMessage),
    Fade(fade::FadeMessage),
    LoadAction,
    ExecuteAction,
    DeleteAction,
    EditAction,
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
    medit: MenuItem,
    mdelete: MenuItem,
    mload: MenuItem,
    mexec: MenuItem,
    mcreate_audio: MenuItem,
    mcreate_fade: MenuItem
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
}
pub struct UITemplate {
    pub pwin: PropertyWindow,
    pub close_btn: Button,
    pub load_btn: Button,
    pub execute_btn: Button,
    pub tx: UISender,
    pub popped_out: bool,
    pub uu: Uuid
}

impl UITemplate {
    pub fn new(uu: Uuid, tx: UISender) -> Self {
        let mut pwin = PropertyWindow::new();
        let close_btn = Button::new_with_mnemonic("_Close");
        let load_btn = Button::new_with_mnemonic("_Load");
        let execute_btn = Button::new_with_mnemonic("_Execute");
        let btn_box = ButtonBox::new(Orientation::Horizontal);
        let popped_out = false;
        btn_box.set_layout(ButtonBoxStyle::Spread);
        btn_box.pack_start(&load_btn, false, false, 0);
        btn_box.pack_start(&execute_btn, false, false, 0);
        pwin.append_button(&close_btn);
        pwin.props_box.pack_start(&btn_box, false, false, 0);
        UITemplate { pwin, close_btn, load_btn, execute_btn, popped_out, tx, uu }
    }
    pub fn get_container(&mut self) -> Option<Widget> {
        if self.pwin.window.is_visible() {
            None
        }
        else {
            if !self.popped_out {
                self.pwin.props_box_box.remove(&self.pwin.props_box);
                self.popped_out = true;
            }
            let swin = ScrolledWindow::new(None, None);
            swin.add(&self.pwin.props_box);
            Some(swin.upcast())
        }
    }
    pub fn edit_separately(&mut self) {
        if self.popped_out {
            self.popped_out = false;
            self.pwin.props_box_box.pack_start(&self.pwin.props_box, true, true, 0);
        }
        self.pwin.window.show_all();
    }
    pub fn bind(&mut self) {
        let uu = self.uu;
        let ref tx = self.tx;
        use self::ActionMessageInner::*;
        self.close_btn.connect_clicked(clone!(tx; |_a| {
            tx.send_internal((uu, CloseButton));
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
        let cur_widget = None;
        let cur_sel = None;
        let mixer = Default::default();
        build!(ActionController using b
               with ctls, opas, tx, mixer, cur_widget, cur_sel
               get view, store, medit, mload, mexec, mdelete, mcreate_audio, mcreate_fade, sidebar)
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
            let iter = self.store.insert_with_values(None, &[0, 1], &[
                &uu.to_string(),
                &action.desc
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
                action_reply_notify!(self, res, "Modifying action", "Action modified.");
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
                self.tx.as_mut().unwrap().send(Command::CreateAction { typ: typ.into() });
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
                    params.url = Some(file.into());
                    let params = ActionParameters::Audio(params);
                    self.tx.as_mut().unwrap()
                        .send(Command::CreateActionWithParams { typ: "audio".into(), params });
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
                DeleteAction => tx.send(Command::DeleteAction { uuid: msg.0 }),
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
