use gtk::prelude::*;
use gtk::{self, Widget, Menu, TreeView, ListStore, SelectionMode, Builder, MenuItem, TreeSelection, TreeIter, TargetEntry, TargetFlags, Stack};
use gtk::Box as GBox;
use gdk::WindowExt;
use gdk;
use gtk::DragContextExtManual;
use uuid::Uuid;
use std::collections::HashMap;
use std::mem;
use std::cell::Cell;
use std::rc::Rc;
use sync::UISender;
use sqa_backend::codec::{Command, Reply};
use sqa_backend::mixer::MixerConf;
use sqa_backend::actions::{ActionParameters, PlaybackState, OpaqueAction};
use sqa_backend::actions::audio::AudioParams;
use messages::Message;
use std::time::Duration;
use widgets::{DurationEntry, DurationEntryMessage};
use glib::signal;

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
    ChangeCurPage(Option<u32>),
}
#[derive(Clone)]
pub enum ActionMessageInner {
    Audio(audio::AudioMessage),
    Fade(fade::FadeMessage),
    LoadAction,
    ExecuteAction,
    DeleteAction,
    ResetAction,
    PauseAction,
    EditAction,
    ChangeName(Option<String>),
    ChangePrewait(Duration),
    Retarget(Uuid),
    CloseButton,
}
impl DurationEntryMessage for ActionMessageInner {
    type Message = ActionMessage;
    type Identifier = Uuid;

    fn on_payload(dur: Duration, id: Uuid) -> Self::Message {
        (id, ActionMessageInner::ChangePrewait(dur))
    }
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
    mload: MenuItem,
    mexec: MenuItem,
    mcreate_audio: MenuItem,
    mcreate_fade: MenuItem,
    drag_notif: GBox,
    cur_page: Option<u32>,
    sel_handler: u64
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
    fn is_dnd_target(&self) -> bool { false }
    fn change_cur_page(&mut self, Option<u32>);
}

#[derive(Clone)]
struct TreeSelectGetter {
    sel: TreeSelection,
    ts: ListStore
}
impl TreeSelectGetter {
    pub fn iter_to_value(&self, ti: TreeIter) -> Option<Uuid> {
        if let Some(v) = self.ts.get_value(&ti, 0).get::<String>() {
            if let Ok(uu) = Uuid::parse_str(&v) {
                return Some(uu);
            }
        }
        None
    }
    pub fn iter_is_dnd_target(&self, ti: TreeIter) -> bool {
        if let Some(v) = self.ts.get_value(&ti, 7).get::<bool>() {
            return v;
        }
        false
    }
    pub fn get(&self) -> Option<Uuid> {
        if let Some((_, ti)) = self.sel.get_selected() {
            return self.iter_to_value(ti)
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
impl ActionController {
    pub fn new(b: &Builder) -> Self {
        let ctls = HashMap::new();
        let opas = HashMap::new();
        let tx = None;
        let cur_page = None;
        let cur_widget = None;
        let cur_sel = None;
        let mixer = Default::default();
        let sel_handler = 0;
        build!(ActionController using b
               with ctls, opas, tx, mixer, cur_widget, cur_sel, cur_page, sel_handler
               get view, store, menu, medit, mload, mexec, mcreate_audio, mcreate_fade, sidebar, drag_notif)
    }
    pub fn bind(&mut self, tx: &UISender) {
        use self::ActionMessageInner::*;
        self.tx = Some(tx.clone());
        self.view.get_selection().set_mode(SelectionMode::Single);
        let tsg = TreeSelectGetter { ts: self.store.clone(), sel: self.view.get_selection() };
        bind_action_menu_items! {
            self, tx, tsg,
            medit => EditAction,
            mexec => ExecuteAction,
            mload => LoadAction
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
        self.view.connect_key_press_event(clone!(tx, tsg; |_, ek| {
            if ek.get_keyval() == gdk::enums::key::Delete {
                if let Some(uu) = tsg.get() {
                    tx.send_internal((uu, DeleteAction));
                    return Inhibit(true);
                }
            }
            Inhibit(false)
        }));
        self.sel_handler = self.view.get_selection().connect_changed(clone!(tx; |_| {
            tx.send_internal(ActionInternalMessage::SelectionChanged);
        }));
        self.mcreate_audio.connect_activate(clone!(tx; |_| {
            tx.send_internal(ActionInternalMessage::Create("audio"));
        }));
        self.mcreate_fade.connect_activate(clone!(tx; |_| {
            tx.send_internal(ActionInternalMessage::Create("fade"));
        }));
        let row_dnd_target = TargetEntry::new("STRING", gtk::TARGET_SAME_WIDGET, 0);
        let dnd_targets = vec![
            TargetEntry::new("text/uri-list", TargetFlags::empty(), 0),
            row_dnd_target.clone()
        ];
        self.view.drag_source_set(gdk::BUTTON1_MASK, &vec![row_dnd_target], gdk::ACTION_COPY | gdk::ACTION_MOVE);
        self.view.drag_dest_set(gtk::DEST_DEFAULT_ALL, &dnd_targets, gdk::ACTION_COPY | gdk::ACTION_MOVE);
        let dn = self.drag_notif.clone();
        let press_coords = Rc::new(Cell::new((0.0, 0.0)));
        let drag_uu: Rc<Cell<Option<Uuid>>> = Rc::new(Cell::new(None));
        self.view.connect_button_press_event(clone!(press_coords; |_, eb| {
            let coords = eb.get_position();
            press_coords.set(coords);
            Inhibit(false)
        }));
        self.view.connect_drag_begin(clone!(dn, tsg, press_coords, drag_uu; |slf, dctx| {
            let (x, y) = press_coords.get();
            let (x, y) = (x.trunc() as _, y.trunc() as _);
            if let Some((Some(path), _, _, _)) = slf.get_path_at_pos(x, y) {
                debug!("dnd: drag begun");
                dn.show_all();

                if let Some(ti) = tsg.ts.get_iter(&path) {
                    if let Some(uu) = tsg.iter_to_value(ti) {
                        debug!("dnd: dragging uu {}", uu);
                        drag_uu.set(Some(uu));
                    }
                }
                if let Some(surf) = slf.create_row_drag_icon(&path) {
                    dctx.drag_set_icon_surface(&surf);
                }
            }
            else {
                /*
                FIXME: This provokes memory unsafety for some reason. Probably a bug in gtk.
                debug!("dnd: cancelling failed drag");
                dctx.drag_cancel();
                */
            }
        }));
        self.view.connect_drag_motion(clone!(drag_uu, tsg; |slf, dctx, x, y, _| {
            use gdk::DragContextExtManual;
            if drag_uu.get().is_some() {
                let (x, y) = slf.convert_widget_to_bin_window_coords(x, y);
                if let Some((Some(path), _, _, _)) = slf.get_path_at_pos(x, y) {
                    if let Some(ti) = tsg.ts.get_iter(&path) {
                        if !tsg.iter_is_dnd_target(ti) {
                            dctx.drag_status(gdk::DragAction::empty(), 0);
                        }
                        else {
                            dctx.drag_status(gdk::ACTION_MOVE, 0);
                        }
                        return Inhibit(false);
                    }
                }
                dctx.drag_status(gdk::DragAction::empty(), 0);
            }
            else {
                dctx.drag_status(gdk::ACTION_COPY, 0);
            }
            Inhibit(false)
        }));
        self.view.connect_drag_end(clone!(drag_uu; |_, _| {
            debug!("dnd: drag ended");
            drag_uu.set(None);
            dn.hide();
        }));
        self.view.connect_drag_data_get(clone!(drag_uu; |_, _, data, _, _| {
            if let Some(uu) = drag_uu.get() {
                debug!("dnd: drag_data_get called for uu {}", uu);
                data.set_text(&format!("{}", uu), -1);
            }
            else {
                debug!("dnd: drag_data_get called, but no uu");
            }
        }));
        self.view.connect_drag_data_received(clone!(tx, tsg; |slf, _, x, y, data, _, _| {
            let uris = data.get_uris();
            debug!("dnd: got uris {:?}", uris);
            if uris.len() == 0 {
                if let Some(txt) = data.get_text() {
                    if let Ok(uu) = txt.parse::<Uuid>() {
                        debug!("dnd: got UUID dropped {}, widget coords ({}, {})", uu, x, y);
                        let (x, y) = slf.convert_widget_to_bin_window_coords(x, y);
                        debug!("dnd: bin window coords ({}, {})", x, y);
                        if let Some((Some(path), _, _, _)) = slf.get_path_at_pos(x, y) {
                            if let Some(ti) = tsg.ts.get_iter(&path) {
                                if let Some(uu2) = tsg.iter_to_value(ti) {
                                    debug!("dnd: was dropped onto {}", uu2);
                                    tx.send_internal((uu2, ActionMessageInner::Retarget(uu)));
                                }
                            }
                        }
                    }
                    else {
                        return;
                    }
                }
                else {
                    return;
                }
            }
            tx.send_internal(ActionInternalMessage::FilesDropped(uris));
        }));

        tx.send_internal(ActionInternalMessage::SelectionChanged);
    }
    fn update_store(&mut self, sel: Option<Uuid>) {
        let tsg = TreeSelectGetter { ts: self.store.clone(), sel: self.view.get_selection() };
        let sel = if sel.is_some() { sel } else {
            signal::signal_handler_block(&tsg.sel, self.sel_handler);
            tsg.get()
        };
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
                Paused(_) => "gtk-media-pause",
                Active(_) => "gtk-media-play",
                Errored(_) => "gtk-dialog-error"
            };
            let mut duration_progress = 0;
            let duration = match action.state {
                Active(Some(ref dur)) | Paused(Some(ref dur)) => {
                    let (elapsed, pos) = dur.elapsed(true);
                    let text = if pos { "T+" } else { "T-" };
                    if pos && dur.est_duration.is_some() {
                        let ed = dur.est_duration.unwrap();
                        let ed = (ed.as_secs() as f32 * 1_000_000_000.0) + ed.subsec_nanos() as f32;
                        let elapsed = (elapsed.as_secs() as f32 * 1_000_000_000.0) + elapsed.subsec_nanos() as f32;
                        let mut perc = ((elapsed / ed) * 100.0).trunc();
                        if perc > 100.0 {
                            perc = 100.0;
                        }
                        duration_progress = perc as u32;
                    }
                    format!("{}{}", text, DurationEntry::format(elapsed, false))
                },
                _ => "".into()
            };
            let is_dnd_target = self.ctls.get(&uu).unwrap().is_dnd_target();
            let iter = self.store.insert_with_values(None, &[
                0, // uuid
                1, // description
                2, // icon-state (playback state icon)
                3, // icon-type (action type icon)
                4, // duration
                6, // duration progress bar (0-100 percent)
                7, // is-dnd-target (can we drop other actions onto this one?)
            ], &[
                &uu.to_string(),
                &action.display_name(),
                &state,
                &typ,
                &duration,
                &duration_progress,
                &is_dnd_target
            ]);
            if let Some(u2) = sel {
                if *uu == u2 {
                    tsg.sel.select_iter(&iter);
                }
            }
        }
        signal::signal_handler_unblock(&tsg.sel, self.sel_handler);
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
            ActionMaybePaused { res, .. } => {
                if let Err(e) = res {
                    let msg = Message::Error(format!("Pausing action failed: {}", e));
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
                let msg = Command::CreateActionWithUuid { typ: typ.into(), uuid };
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
                    let msg = Command::CreateActionWithExtras { typ: "audio".into(), params: params, uuid };
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
        if let Some(ctl) = self.ctls.get_mut(&msg.0) {
            let tx = self.tx.as_mut().unwrap();
            use self::ActionMessageInner::*;
            match msg.1 {
                LoadAction => tx.send(Command::LoadAction { uuid: msg.0 }),
                ExecuteAction => tx.send(Command::ExecuteAction { uuid: msg.0 }),
                ResetAction => tx.send(Command::ResetAction { uuid: msg.0 }),
                DeleteAction => tx.send(Command::DeleteAction { uuid: msg.0 }),
                PauseAction => tx.send(Command::PauseAction { uuid: msg.0 }),
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
                    trace!("ChangeName({:?}) called", name);
                    if let Some(opa) = self.opas.get_mut(&msg.0) {
                        opa.meta.name = name;
                        tx.send(Command::UpdateActionMetadata { uuid: msg.0, meta: opa.meta.clone() });
                    }
                },
                ChangePrewait(dur) => {
                    if let Some(opa) = self.opas.get_mut(&msg.0) {
                        opa.meta.prewait = dur;
                        tx.send(Command::UpdateActionMetadata { uuid: msg.0, meta: opa.meta.clone() });
                    }
                },
                Retarget(uu) => ctl.on_selection_finished(uu),
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
