use gtk::prelude::*;
use gtk::Widget;
use super::{ActionMessageInner, ActionMessage, OpaqueAction, UISender, ActionUI, UITemplate};
use widgets::{SliderBox, Faded, SliderMessage, FadedSliderMessage};
use uuid::Uuid;
use sqa_backend::codec::Command;
use sqa_backend::actions::ActionParameters;
use sqa_backend::actions::fade::FadeParams;

pub enum FadeMessage {
    Slider(usize, FadedSliderMessage)
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

pub struct FadeUI {
    temp: UITemplate,
    params: FadeParams,
    sb: SliderBox<Faded, FadeMessage>
}
impl FadeUI {
    pub fn new(uu: Uuid, tx: UISender) -> Self {
        let temp = UITemplate::new(uu, tx.clone());
        let sb = SliderBox::new(0, 0, &tx, uu);
        let params = Default::default();
        temp.pwin.props_box.pack_start(&sb.grid, false, true, 5);
        let mut ctx = FadeUI { temp, params, sb };
        ctx.bind();
        ctx
    }
    fn bind(&mut self) {
        self.temp.bind();
    }
    fn on_new_parameters(&mut self, p: &FadeParams) {
        trace!("fade: new parameters {:?}", p);
        self.params = p.clone();
        if p.fades.len() != self.sb.n_sliders() {
            self.sb.grid.destroy();
            self.sb = SliderBox::new(p.fades.len(), 0, &self.temp.tx, self.temp.uu);
            self.temp.pwin.props_box.pack_start(&self.sb.grid, false, true, 5);
            self.temp.pwin.props_box.show_all();
        }
        let mut fades = p.fades.clone();
        fades.insert(0, p.fade_master.clone());
        self.sb.update_values(fades);
    }
    fn apply_changes(&mut self, params: FadeParams) {
        trace!("fade: sending update {:?}", params);
        self.temp.tx.send(Command::UpdateActionParams {
            uuid: self.temp.uu,
            params: ActionParameters::Fade(params)
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
                    let mut params = self.params.clone();
                    if ch == 0 {
                        trace!("fade: slider cb: ch {} val {:?}", ch, val);
                        params.fade_master = val;
                    }
                    else if let Some(v) = params.fades.get_mut(ch-1) {
                        *v = val;
                    }
                    self.apply_changes(params);
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
}
