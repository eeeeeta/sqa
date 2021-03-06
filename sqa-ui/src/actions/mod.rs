use gtk::prelude::*;
use gtk::{self, Widget, Menu, TreeView, ListStore, SelectionMode, Builder, MenuItem, TreeSelection, TreeIter, TargetEntry, TargetFlags, Stack};
use gtk::Box as GBox;
use gdk::WindowExt;
use gdk;
use gtk::DragContextExtManual;
use uuid::Uuid;
use std::collections::{HashSet, HashMap};
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
use copy::CopyPasteMessage;
use sqa_backend::waveform::WaveformReply;

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
    CopyPaste(CopyPasteMessage),
    Rebuild,
    DndReorderBefore,
    DndReorderAfter,
    DndRetarget,
    CloseDndMenu,
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
    OpenDndMenu(Uuid),
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
    order: Vec<Uuid>,
    cur_sel: Option<SelectionDetails>,
    cur_dnd: Option<(Uuid, Uuid)>,
    clipboard: Option<OpaqueAction>,
    menu: Menu,
    medit: MenuItem,
    mload: MenuItem,
    mreset: MenuItem,
    mpause: MenuItem,
    mexec: MenuItem,
    mdelete: MenuItem,
    mrebuild: MenuItem,
    mcreate_audio: MenuItem,
    mcreate_fade: MenuItem,
    dnd_menu: Menu,
    mdnd_retarget: MenuItem,
    mdnd_reorder_before: MenuItem,
    mdnd_reorder_after: MenuItem,
    mdnd_close: MenuItem,
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
    fn on_waveform_reply(&mut self, _uu: Uuid, _rpl: &WaveformReply) {}
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
        build!(ActionController using b
               default tx, cur_page, cur_widget, cur_sel, clipboard, mixer, sel_handler,
                       ctls, opas, order, cur_dnd
               get     view, store, menu, medit, mload, mexec, mdelete, mrebuild, mreset,
                       mpause, mcreate_audio, mcreate_fade, sidebar, drag_notif,
                       dnd_menu, mdnd_retarget, mdnd_reorder_before, mdnd_reorder_after,
                       mdnd_close)
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
            mdelete => DeleteAction,
            mreset => ResetAction,
            mpause => PauseAction,
            mload => LoadAction
        }
        use self::ActionInternalMessage::*;
        bind_menu_items! {
            self, tx,
            mcreate_audio => Create("audio"),
            mcreate_fade => Create("fade"),
            mrebuild => Rebuild,
            mdnd_retarget => DndRetarget,
            mdnd_reorder_before => DndReorderBefore,
            mdnd_reorder_after => DndReorderAfter,
            mdnd_close => CloseDndMenu
        }
        let menu = self.menu.clone();
        self.dnd_menu.connect_focus_out_event(clone!(tx; |_, _| {
            tx.send_internal(ActionInternalMessage::CloseDndMenu);
            Inhibit(false)
        }));
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
                    if tsg.ts.get_iter(&path).is_some() {
                        dctx.drag_status(gdk::ACTION_MOVE, 0);
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
                                    tx.send_internal((uu2, ActionMessageInner::OpenDndMenu(uu)));
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
        trace!("update_store(sel = {:?}) called", sel);
        let tsg = TreeSelectGetter { ts: self.store.clone(), sel: self.view.get_selection() };
        let sel = if sel.is_some() { sel } else {
            signal::signal_handler_block(&tsg.sel, self.sel_handler);
            tsg.get()
        };
        self.store.clear();
        trace!("order looks like {:?}", self.order);
        for uu in self.order.iter() {
            let action;
            if let Some(a) = self.opas.get(uu) {
                action = a;
            }
            else {
                warn!("UUID {} in order, but not in opas!", uu);
                continue;
            }
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
        // Helpful Warning: you can't just add new Replies here and expect it to work!
        // The list of Replies that get forwarded here also needs to be updated.
        // Go check connection.rs and update it.
        match r {
            UpdateActionInfo { uuid, data } => self.on_action_info(uuid, data),
            UpdateActionDeleted { uuid } => {
                self.opas.remove(&uuid);
                self.ctls.remove(&uuid);
                self.tx.as_mut().unwrap()
                    .send_internal(Message::Statusbar("Action deleted.".into()));
            },
            UpdateOrder { order } => self.order = order,
            ReplyActionList { list, order } => {
                let mut to_remove = self.opas.iter().map(|(&k, _)| k).collect::<HashSet<_>>();
                for (uu, oa) in list {
                    self.on_action_info(uu, oa);
                    to_remove.remove(&uu);
                }
                for uu in to_remove {
                    self.opas.remove(&uu);
                    self.ctls.remove(&uu);
                }
                self.order = order;
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
            ActionReordered { res, .. } => {
                action_reply_notify!(self, res, "Reordering action", "Action reordered.");
            },
            WaveformGenerated { uuid, res } => {
                debug!("Got waveform reply for uuid {}", uuid);
                match res {
                    Ok(rpl) => {
                        for (_, ctl) in self.ctls.iter_mut() {
                            ctl.on_waveform_reply(uuid, &rpl);
                        }
                    },
                    Err(e) => {
                        self.tx.as_mut().unwrap()
                            .send_internal(Message::Error(format!("Waveform generation failed: {}", e)));
                    }
                }
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
                self.mdelete.set_sensitive(activated);
                self.mreset.set_sensitive(activated);
                self.mpause.set_sensitive(activated);
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
            },
            CopyPaste(msg) => {
                use self::CopyPasteMessage::*;
                debug!("got copypaste: {:?}", msg);
                match msg {
                    x @ Copy | x @ Cut => {
                        let tsg = TreeSelectGetter { ts: self.store.clone(), sel: self.view.get_selection() };
                        if let Some(uu) = tsg.get() {
                            if let Some(opa) = self.opas.get(&uu) {
                                self.clipboard = Some(opa.clone());
                            }
                            if let Cut = x {
                                self.tx.as_mut().unwrap()
                                    .send_internal((uu, ActionMessageInner::DeleteAction));
                            }
                        }
                    },
                    Paste => {
                        if let Some(opa) = self.clipboard.clone() {
                            let typ = opa.typ().into();
                            let OpaqueAction { mut uu, params, meta, .. } = opa;
                            if self.opas.get(&uu).is_some() {
                                uu = Uuid::new_v4();
                            }
                            self.tx.as_mut().unwrap()
                                .send(Command::ReviveAction { typ, uuid: uu, params, meta });
                        }
                    }
                };
            },
            Rebuild => {
                self.tx.as_mut().unwrap()
                    .send(Command::ActionList);
            },
            DndReorderBefore => {
                if let Some((from, to)) = self.cur_dnd.take() {
                    debug!("dnd: reordering {} before {}", from, to);
                    if let Some(_) = self.order.iter().position(|&uu| uu == from) {
                        if let Some(tpos) = self.order.iter().position(|&uu| uu == to) {
                            let tpos = tpos.saturating_sub(1);
                            self.tx.as_mut().unwrap()
                                .send(Command::ReorderAction { uuid: from, new_pos: tpos });
                        }
                        else {
                            warn!("dnd: dest {} doesn't exist in order", to);
                        }
                    }
                    else {
                        warn!("dnd: source {} doesn't exist in order", to);
                    }
                }
            },
            DndReorderAfter => {
                if let Some((from, to)) = self.cur_dnd.take() {
                    debug!("dnd: reordering {} after {}", from, to);
                    if let Some(_) = self.order.iter().position(|&uu| uu == from) {
                        if let Some(tpos) = self.order.iter().position(|&uu| uu == to) {
                            let tpos = if (tpos + 2) >= self.order.len() { tpos } else { tpos + 1 };
                            self.tx.as_mut().unwrap()
                                .send(Command::ReorderAction { uuid: from, new_pos: tpos });
                        }
                        else {
                            warn!("dnd: dest {} doesn't exist in order", to);
                        }
                    }
                    else {
                        warn!("dnd: source {} doesn't exist in order", to);
                    }
                }
            },
            DndRetarget => {
                if let Some((from, to)) = self.cur_dnd.take() {
                    debug!("dnd: action {} target now = {}", to, from);
                    if let Some(ctl) = self.ctls.get_mut(&to) {
                        ctl.on_selection_finished(from);
                    }
                    else {
                        warn!("dnd: target {} doesn't exist", to);
                    }
                }
            },
            CloseDndMenu => {
                self.dnd_menu.hide();
                self.cur_dnd = None;
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
                OpenDndMenu(uu) => {
                    debug!("dnd: showing menu");
                    self.cur_dnd = Some((uu, msg.0));
                    self.mdnd_retarget.set_sensitive(ctl.is_dnd_target());
                    self.dnd_menu.popup_easy(0, 0);
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
