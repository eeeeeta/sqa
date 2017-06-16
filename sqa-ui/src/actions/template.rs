use gtk::prelude::*;
use gtk::{Button, ButtonBox, ButtonBoxStyle, Box, Label, Orientation, Notebook, Widget, ScrolledWindow};
use widgets::PropertyWindow;
use sync::UISender;
use uuid::Uuid;
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
    pub notebk_tabs: Vec<ActionTab>,
    pub close_btn: Button,
    pub load_btn: Button,
    pub execute_btn: Button,
    pub tx: UISender,
    pub popped_out: bool,
    pub uu: Uuid
}

impl UITemplate {
    pub fn new(uu: Uuid, tx: UISender) -> Self {
        let mut ret = UITemplate {
            pwin: PropertyWindow::new(),
            close_btn: Button::new_with_mnemonic("_Close"),
            load_btn: Button::new_with_mnemonic("_Load"),
            execute_btn: Button::new_with_mnemonic("_Execute"),
            notebk: Notebook::new(),
            notebk_tabs: Vec::new(),
            popped_out: false,
            tx, uu
        };
        let btn_box = ButtonBox::new(Orientation::Horizontal);
        btn_box.set_layout(ButtonBoxStyle::Spread);
        btn_box.pack_start(&ret.load_btn, false, false, 0);
        btn_box.pack_start(&ret.execute_btn, false, false, 0);
        ret.pwin.append_button(&ret.close_btn);
        let basics_tab = ret.add_tab();
        basics_tab.label.set_markup("Basics");
        basics_tab.container.pack_start(&btn_box, false, false, 0);
        ret.pwin.props_box.pack_start(&ret.notebk, true, true, 0);
        ret
    }
    pub fn add_tab(&mut self) -> ActionTab {
        let bx = Box::new(Orientation::Vertical, 0);
        bx.set_margin_left(5);
        bx.set_margin_right(5);
        bx.set_margin_top(5);
        bx.set_margin_bottom(5);
        let lbl = Label::new(None);
        let at = ActionTab { container: bx, label: lbl };
        self.notebk.insert_page(&at.container, Some(&at.label), None);
        self.notebk_tabs.push(at.clone());
        at
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
    pub fn change_cur_page(&mut self, cp: Option<u32>) {
        self.notebk.set_current_page(cp);
    }
    pub fn bind(&mut self) {
        let uu = self.uu;
        let ref tx = self.tx;
        use super::ActionMessageInner::*;
        self.close_btn.connect_clicked(clone!(tx; |_a| {
            tx.send_internal((uu, CloseButton));
        }));
        self.load_btn.connect_clicked(clone!(tx; |_a| {
            tx.send_internal((uu, LoadAction));
        }));
        self.execute_btn.connect_clicked(clone!(tx; |_a| {
            tx.send_internal((uu, ExecuteAction));
        }));
        self.notebk.connect_switch_page(clone!(tx; |_, _, pg| {
            tx.send_internal(super::ActionInternalMessage::ChangeCurPage(Some(pg)));
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
