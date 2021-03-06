use gtk::prelude::*;
use gtk::{Button, ButtonBox, ButtonBoxStyle, Box, Label, Image, Orientation, Notebook, Widget, ScrolledWindow, Entry, ListBox, SelectionMode};
use widgets::{PropertyWindow, DurationEntry};
use std::collections::HashMap;
use sync::UISender;
use super::ActionMessageInner;
use uuid::Uuid;
use glib::signal;
use sqa_backend::actions::{PlaybackState, OpaqueAction};

#[derive(Clone)]
pub struct ActionTab {
    pub container: Box,
    pub label: Label
}
impl ActionTab {
    pub fn append_property<T: IsA<Widget>>(&self, text: &str, prop: &T) -> Label {
        use gtk::Align;
        let label = Label::new(None);
        label.set_markup(text);
        label.set_halign(Align::Start);
        let bx = Box::new(Orientation::Horizontal, 0);
        bx.pack_start(&label, false, true, 5);
        bx.pack_end(prop, true, true, 5);
        self.container.pack_start(&bx, false, true, 5);
        label
    }
}
pub struct UITemplate {
    pub pwin: PropertyWindow,
    pub notebk: Notebook,
    pub notebk_tabs: HashMap<&'static str, ActionTab>,
    pub close_btn: Button,
    pub pause_btn: Button,
    pub load_btn: Button,
    pub execute_btn: Button,
    pub reset_btn: Button,
    pub name_ent: Entry,
    pub name_ent_handler: u64,
    pub prewait_ent: DurationEntry,
    pub tx: UISender,
    pub popped_out: bool,
    pub errors_list: ListBox,
    pub uu: Uuid
}
macro_rules! bind_buttons {
    ($self:ident, $tx:ident, $uu:ident, $($name:ident => $res:ident),*) => {
        $(
            $self.$name.connect_clicked(clone!($tx; |_| {
                $tx.send_internal(($uu, $res));
            }));
        )*
    }
}
impl UITemplate {
    pub fn new(uu: Uuid, tx: UISender) -> Self {
        let pwin = PropertyWindow::new_with_handlers("Edit action", clone!(tx; |slf, _| {
            trace!("uitemplate separate window closed");
            slf.hide();
            tx.send_internal(super::ActionInternalMessage::SelectionChanged);
            Inhibit(true)
        }));
        let mut ret = UITemplate {
            pwin: pwin,
            close_btn: Button::new_with_mnemonic("_Close"),
            pause_btn: Button::new_with_mnemonic("_Pause"),
            load_btn: Button::new_with_mnemonic("_Load"),
            execute_btn: Button::new_with_mnemonic("_Execute"),
            reset_btn: Button::new_with_mnemonic("_Reset"),
            notebk: Notebook::new(),
            notebk_tabs: HashMap::new(),
            name_ent: Entry::new(),
            name_ent_handler: 0,
            prewait_ent: DurationEntry::new(),
            errors_list: ListBox::new(),
            popped_out: false,
            tx, uu
        };
        let btn_box = ButtonBox::new(Orientation::Horizontal);
        btn_box.set_layout(ButtonBoxStyle::End);
        btn_box.pack_start(&ret.load_btn, false, false, 5);
        btn_box.pack_start(&ret.execute_btn, false, false, 5);
        btn_box.pack_start(&ret.pause_btn, false, false, 5);
        btn_box.pack_start(&ret.reset_btn, false, false, 5);
        ret.load_btn.set_always_show_image(true);
        ret.load_btn.set_image(&Image::new_from_stock("gtk-home", 4));
        ret.execute_btn.set_always_show_image(true);
        ret.execute_btn.set_image(&Image::new_from_stock("gtk-media-play", 4));
        ret.reset_btn.set_always_show_image(true);
        ret.reset_btn.set_image(&Image::new_from_stock("gtk-refresh", 4));
        ret.pause_btn.set_always_show_image(true);
        ret.pause_btn.set_image(&Image::new_from_stock("gtk-media-pause", 4));
        ret.pwin.append_button(&ret.close_btn);
        let basics_tab = ret.add_tab("Basics");
        let errors_tab = ret.add_tab("Errors");
        basics_tab.append_property("Controls", &btn_box);
        basics_tab.append_property("Name", &ret.name_ent);
        basics_tab.append_property("Pre-wait", &*ret.prewait_ent);
        errors_tab.container.pack_start(&ret.errors_list, false, false, 0);
        ret.errors_list.set_selection_mode(SelectionMode::None);
        ret.pwin.props_box.pack_start(&ret.notebk, true, true, 0);
        ret
    }
    pub fn add_tab(&mut self, id: &'static str) -> ActionTab {
        let bx = Box::new(Orientation::Vertical, 0);
        bx.set_margin_left(5);
        bx.set_margin_right(5);
        bx.set_margin_top(5);
        bx.set_margin_bottom(5);
        let lbl = Label::new(None);
        lbl.set_markup(id);
        let at = ActionTab { container: bx, label: lbl };
        self.notebk.insert_page(&at.container, Some(&at.label), None);
        self.notebk_tabs.insert(id, at.clone());
        trace!("inserting tab '{}' for act {}", id, self.uu);
        at
    }
    pub fn get_tab(&self, id: &'static str) -> &ActionTab {
        trace!("getting tab '{}' for act {}", id, self.uu);
        self.notebk_tabs.get(id).expect(&format!("failed to get tab '{}' for act {}", id, self.uu))
    }
    pub fn get_container(&mut self) -> Option<Widget> {
        if self.pwin.is_visible() {
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
        self.pwin.present();
    }
    pub fn change_cur_page(&mut self, cp: Option<u32>) {
        self.notebk.set_current_page(cp);
    }
    pub fn bind(&mut self) {
        let uu = self.uu;
        let ref tx = self.tx;
        use super::ActionMessageInner::*;
        bind_buttons! {
            self, tx, uu,
            close_btn => CloseButton,
            load_btn => LoadAction,
            execute_btn => ExecuteAction,
            pause_btn => PauseAction,
            reset_btn => ResetAction
        };
        self.notebk.connect_switch_page(clone!(tx; |_, _, pg| {
            tx.send_internal(super::ActionInternalMessage::ChangeCurPage(Some(pg)));
        }));
        self.name_ent_handler = self.name_ent.connect_changed(clone!(tx; |slf| {
            let mut txt = slf.get_text();
            trace!("name entry changed, text {:?}", txt);
            if txt.is_some() {
                if txt.as_ref().unwrap() == "" {
                    txt = None;
                }
            }
            tx.send_internal((uu, ChangeName(txt)));
        }));
        self.prewait_ent.bind::<ActionMessageInner>(tx, uu);
    }
    pub fn box_for_errors_list(msg: &str) -> Box {
        let bx = Box::new(Orientation::Horizontal, 5);
        let img = Image::new_from_icon_name("gtk-dialog-error", 4);
        let lbl = Label::new(None);
        lbl.set_markup(msg);
        bx.pack_start(&img, false, false, 0);
        bx.pack_start(&lbl, false, false, 0);
        bx
    }
    pub fn on_update(&mut self, p: &OpaqueAction) {
        playback_state_update(p, &mut self.pwin);
        signal::signal_handler_block(&self.name_ent, self.name_ent_handler);
        self.name_ent.set_placeholder_text(&p.desc as &str);
        if !self.name_ent.has_focus() {
            trace!("setting name entry text to {:?}", p.meta.name);
            self.name_ent.set_text(p.meta.name.as_ref().map(|s| s as &str).unwrap_or(""));
        }
        signal::signal_handler_unblock(&self.name_ent, self.name_ent_handler);
        self.prewait_ent.set(p.meta.prewait);
        for child in self.errors_list.get_children() {
            self.errors_list.remove(&child);
        }
        if let PlaybackState::Unverified(ref errs) = p.state {
            self.get_tab("Errors").label.set_markup(&format!("Errors ({})", errs.len()));
            for err in errs {
                let bx = Self::box_for_errors_list(&format!("{}: {}", err.name, err.err));
                self.errors_list.add(&bx);
                bx.show_all();
            }
            self.load_btn.set_sensitive(false);
            self.execute_btn.set_sensitive(false);
        }
        else if let PlaybackState::Errored(ref err) = p.state {
            self.get_tab("Errors").label.set_markup("Errors (!!)");
            let bx = Self::box_for_errors_list(
                &format!("The following fatal error occurred, causing the action to stop:\n\t{}\nBecause of this, this action is in an inconsistent state and must be reset to continue.", err));
            let reset_btn = Button::new_with_mnemonic("_Reset");
            reset_btn.set_always_show_image(true);
            reset_btn.set_image(&Image::new_from_stock("gtk-refresh", 4));
            let tx = self.tx.clone();
            let uu = self.uu;
            reset_btn.connect_clicked(move |_| {
                tx.send_internal((uu, ActionMessageInner::ResetAction));
            });
            bx.pack_end(&reset_btn, false, false, 0);
            self.errors_list.add(&bx);
            bx.show_all();
            self.load_btn.set_sensitive(false);
            self.execute_btn.set_sensitive(false);
        }
        else {
            self.load_btn.set_sensitive(true);
            self.execute_btn.set_sensitive(true);
            self.get_tab("Errors").label.set_markup("Errors");
        }
    }
}
pub fn playback_state_update(p: &OpaqueAction, pwin: &mut PropertyWindow) {
    use self::PlaybackState::*;
    let desc = p.display_name();
    match p.state {
        Inactive => pwin.update_header(
            "gtk-media-stop",
            desc,
            "Action inactive."
        ),
        Unverified(ref errs) => pwin.update_header(
            "gtk-dialog-error",
            desc,
            format!("Action incomplete: {} errors are present.", errs.len())
        ),
        Loading => pwin.update_header(
            "gtk-refresh",
            desc,
            "Action loading..."
        ),
        Loaded => pwin.update_header(
            "gtk-home",
            desc,
            "Action loaded."
        ),
        Paused(_) => pwin.update_header(
            "gtk-media-pause",
            desc,
            "Action paused."
        ),
        Active(_) => pwin.update_header(
            "gtk-media-play",
            desc,
            "Action active."
        ),
        _ => {}
    }
}
