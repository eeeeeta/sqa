use gtk::prelude::*;
use gtk::{Button, Widget};
use super::{ActionMessageInner, ActionInternalMessage, ActionMessage, OpaqueAction, UISender, ActionUI, UITemplate};
use std::time::Duration;
use widgets::{SliderBox, Faded, DurationEntry, DurationEntryMessage, SliderMessage, FadedSliderMessage};
use uuid::Uuid;
use std::cell::Cell;
use std::rc::Rc;
use std::collections::HashMap;
use sqa_backend::codec::Command;
use sqa_backend::actions::ActionParameters;
use sqa_backend::actions::fade::FadeParams;

#[derive(Clone)]
pub enum FadeMessage {
    Slider(usize, FadedSliderMessage),
    DurationModified(Duration)
}
impl SliderMessage<Faded> for FadeMessage {
    type Message = ActionMessage;
    type Identifier = Uuid;

    fn on_payload(ch: usize, data: FadedSliderMessage, id: Uuid) -> Self::Message {
        use self::ActionMessageInner::*;
        use self::FadeMessage::*;
        (id, Fade(Slider(ch, data)))
    }
}
impl DurationEntryMessage for FadeMessage {
    type Message = ActionMessage;
    type Identifier = Uuid;

    fn on_payload(dur: Duration, id: Uuid) -> Self::Message {
        use self::ActionMessageInner::*;
        use self::FadeMessage::*;
        (id, Fade(DurationModified(dur)))
    }
}
pub struct FadeUI {
    temp: UITemplate,
    sel: Button,
    params: FadeParams,
    dur: DurationEntry,
    sb: SliderBox<Faded, FadeMessage>,
    selecting: Rc<Cell<bool>>,
    actionlist: HashMap<Uuid, OpaqueAction>,
    tx: UISender
}
impl FadeUI {
    pub fn new(uu: Uuid, tx: UISender) -> Self {
        let mut temp = UITemplate::new(uu, tx.clone());
        let sb = SliderBox::new(0, 0, &tx, uu);
        let params = Default::default();
        let dur = DurationEntry::new();
        let sel = Button::new_with_label("[choose...]");
        let selecting = Rc::new(Cell::new(false));
        let actionlist = HashMap::new();
        let patch = temp.add_tab();
        let fade = temp.add_tab();
        patch.label.set_markup("Levels &amp; Patch");
        fade.label.set_markup("Fade Properties");
        temp.notebk_tabs[0].append_property("Target", &sel);
        fade.append_property("Duration", &*dur);
        patch.container.pack_start(&sb.grid, false, true, 5);
        let mut ctx = FadeUI { temp, params, sb, sel, tx, selecting, actionlist, dur };
        ctx.bind();
        ctx
    }
    fn bind(&mut self) {
        self.temp.bind();
        let ref tx = self.tx;
        let ref selecting = self.selecting;
        let uu = self.temp.uu;
        self.dur.bind::<FadeMessage>(tx, uu);
        self.sel.connect_clicked(clone!(tx, selecting; |slf| {
            if selecting.get() {
                tx.send_internal(ActionInternalMessage::CancelSelection);
            }
            else {
                slf.set_label("Choose an action above [click here to cancel]");
                tx.send_internal(ActionInternalMessage::BeginSelection(uu));
                selecting.set(true);
            }
        }));
    }
    fn on_new_parameters(&mut self, p: &FadeParams) {
        trace!("fade: new parameters {:?}", p);
        self.params = p.clone();
        if let Some(uu) = p.target {
            if let Some(opa) = self.actionlist.get(&uu) {
                self.sel.set_label(&opa.desc);
            }
            else {
                self.sel.set_label(&format!("{}", uu));
            }
        }
        else {
            self.sel.set_label("[choose...]");
        }
        self.dur.set(p.dur);
        if p.fades.len() != self.sb.n_sliders() {
            self.sb.grid.destroy();
            self.sb = SliderBox::new(p.fades.len(), 0, &self.temp.tx, self.temp.uu);
            self.temp.notebk_tabs[1].container.pack_start(&self.sb.grid, false, true, 5);
            self.temp.notebk_tabs[1].container.show_all();
        }
        let mut fades = p.fades.clone();
        fades.insert(0, p.fade_master.clone());
        self.sb.update_values(fades);
    }
    fn apply_changes(&mut self) {
        trace!("fade: sending update {:?}", self.params);
        self.temp.tx.send(Command::UpdateActionParams {
            uuid: self.temp.uu,
            params: ActionParameters::Fade(self.params.clone())
        });
    }
}

impl ActionUI for FadeUI {
    fn on_update(&mut self, p: &OpaqueAction) {
        self.temp.on_update(p);
        if let ActionParameters::Fade(ref pars) = p.params {
            self.on_new_parameters(pars);
        }
    }
    fn on_message(&mut self, m: ActionMessageInner) {
        if let ActionMessageInner::Fade(m) = m {
            use self::FadeMessage::*;
            match m {
                Slider(ch, val) => {
                    if ch == 0 {
                        trace!("fade: slider cb: ch {} val {:?}", ch, val);
                        self.params.fade_master = val;
                    }
                    else if let Some(v) = self.params.fades.get_mut(ch-1) {
                        *v = val;
                    }
                    self.apply_changes();
                },
                DurationModified(dur) => {
                    self.params.dur = dur;
                    trace!("dur cb: new dur {:?}", dur);
                    self.apply_changes();
                }
            }
        }
    }
    fn close_window(&mut self) {
        self.temp.pwin.window.hide();
    }
    fn edit_separately(&mut self) {
        self.temp.edit_separately();
    }
    fn get_container(&mut self) -> Option<Widget> {
        self.temp.get_container()
    }
    fn on_selection_finished(&mut self, sel: Uuid) {
        trace!("selected {}", sel);
        self.params.target = Some(sel);
        self.apply_changes();
        self.selecting.set(false);
    }
    fn on_selection_cancelled(&mut self) {
        trace!("selection cancelled");
        let p = self.params.clone();
        self.on_new_parameters(&p);
        self.selecting.set(false);
    }
    fn on_action_list(&mut self, l: &HashMap<Uuid, OpaqueAction>) {
        trace!("got new actionlist");
        self.actionlist = l.clone();
        let p = self.params.clone();
        self.on_new_parameters(&p);
    }
    fn change_cur_page(&mut self, cp: Option<u32>) {
        self.temp.change_cur_page(cp)
    }
}
