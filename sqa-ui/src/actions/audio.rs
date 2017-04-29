use gtk::prelude::*;
use gtk::{Builder, Button};
use super::{ActionUI, OpaqueAction, ActionMessageInner};
use sync::UISender;
use uuid::Uuid;
use widgets::{PropertyWindow, FallibleEntry};
use sqa_backend::codec::Command;
use sqa_backend::actions::{ActionParameters, ActionController, PlaybackState};
use sqa_backend::actions::audio::Controller as AudioController;
use sqa_backend::actions::audio::AudioParams;

pub enum AudioMessage {
    ApplyButton,
    OkButton,
    CancelButton
}
pub struct AudioUI {
    pwin: PropertyWindow,
    fe: FallibleEntry,
    params: AudioParams,
    apply_btn: Button,
    ok_btn: Button,
    cancel_btn: Button,
    tx: UISender,
    uu: Uuid
}

impl AudioUI {
    pub fn new(b: &Builder, uu: Uuid, tx: UISender) -> Self {
        let mut pwin = PropertyWindow::new();
        let fe = FallibleEntry::new();
        let apply_btn = Button::new_with_mnemonic("_Apply");
        let ok_btn = Button::new_with_mnemonic("_OK");
        let cancel_btn = Button::new_with_mnemonic("_Cancel");
        let params = Default::default();
        pwin.append_property("Filename", &*fe);
        pwin.append_button(&ok_btn);
        pwin.append_button(&cancel_btn);
        pwin.append_button(&apply_btn);
        let mut ctx = AudioUI { pwin, fe, params, apply_btn, ok_btn, cancel_btn, uu, tx };
        ctx.bind();
        ctx
    }
    fn bind(&mut self) {
        let uu = self.uu;
        let ref tx = self.tx;
        use self::ActionMessageInner::Audio;
        use self::AudioMessage::*;
        self.apply_btn.connect_clicked(clone!(tx; |_a| {
            tx.send_internal((uu, Audio(ApplyButton)));
        }));
        self.ok_btn.connect_clicked(clone!(tx; |_a| {
            tx.send_internal((uu, Audio(OkButton)));
        }));
        self.cancel_btn.connect_clicked(clone!(tx; |_a| {
            tx.send_internal((uu, Audio(CancelButton)));
        }));
        self.fe.on_enter(clone!(tx; |_a| {
            tx.send_internal((uu, Audio(ApplyButton)));
        }));
    }
    fn on_new_parameters(&mut self, p: &AudioParams) {
        self.fe.set_text(p.url.as_ref().map(|x| x as &str).unwrap_or(""));
        self.params = p.clone();
    }
}
impl ActionUI for AudioUI {
    fn on_update(&mut self, p: &OpaqueAction) {
        let ActionParameters::Audio(ref pars) = p.params;
        self.on_new_parameters(pars);

        super::playback_state_update(p, &mut self.pwin);
        self.fe.reset_error();
        if let PlaybackState::Unverified(ref errs) = p.state {
            for err in errs {
                if err.name == "url" {
                    self.fe.throw_error(&err.err);
                }
            }
        }
    }
    fn on_message(&mut self, m: ActionMessageInner) {
        let ActionMessageInner::Audio(m) = m;
        use self::AudioMessage::*;
        match m {
            x @ ApplyButton | x @ OkButton => {
                let url = self.fe.get_text();
                let url = if url == "" { None } else { Some(url.into()) };
                let patch = self.params.patch.clone();
                let params = AudioParams { url, patch };
                self.tx.send(Command::UpdateActionParams {
                    uuid: self.uu,
                    params: ActionParameters::Audio(params)
                });
                if let OkButton = x {
                    self.pwin.window.hide();
                }
            },
            CancelButton => {
                let pc = self.params.clone();
                self.on_new_parameters(&pc);
                self.pwin.window.hide();
            },
        }
    }
    fn show(&mut self) {
        self.pwin.window.show_all();
    }
}
