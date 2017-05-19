use gtk::prelude::*;
use gtk::{Builder, FileChooserAction, FileChooserButton, Widget};
use super::{ActionUI, OpaqueAction, ActionUIMessage, UITemplate, ActionMessageInner};
use sync::UISender;
use uuid::Uuid;
use widgets::{SliderBox, FallibleEntry, SliderMessage, SliderDetail};
use sqa_backend::codec::Command;
use sqa_backend::mixer::MixerConf;
use sqa_backend::actions::{ActionParameters, ActionController, PlaybackState};
use sqa_backend::actions::audio::Controller as AudioController;
use sqa_backend::actions::audio::AudioParams;

pub enum AudioMessage {
    ApplyButton,
    OkButton,
    CancelButton,
    VolChanged(usize, f32),
    PatchChanged(usize, usize)
}
impl SliderMessage for AudioMessage {
    type Message = (Uuid, ActionMessageInner);
    type Identifier = Uuid;

    fn vol_changed(ch: usize, val: f64, id: Uuid) -> Self::Message {
        use self::ActionMessageInner::*;
        use self::AudioMessage::*;
        (id, Audio(VolChanged(ch, val as _)))
    }
    fn patch_changed(ch: usize, patch: usize, id: Uuid) -> Self::Message {
        use self::ActionMessageInner::*;
        use self::AudioMessage::*;
        (id, Audio(PatchChanged(ch, patch)))
    }
}
impl ActionUIMessage for AudioMessage {
    fn apply() -> ActionMessageInner {
        ActionMessageInner::Audio(AudioMessage::ApplyButton)
    }
    fn ok() -> ActionMessageInner {
        ActionMessageInner::Audio(AudioMessage::OkButton)
    }
    fn cancel() -> ActionMessageInner {
        ActionMessageInner::Audio(AudioMessage::CancelButton)
    }
}
pub struct AudioUI {
    file: FileChooserButton,
    params: AudioParams,
    temp: UITemplate,
    cnf: MixerConf,
    sb: SliderBox
}

impl AudioUI {
    pub fn new(b: &Builder, uu: Uuid, tx: UISender) -> Self {
        let file = FileChooserButton::new("Audio file", FileChooserAction::Open);
        let mut temp = UITemplate::new(uu, tx.clone());
        let params = Default::default();
        let cnf = Default::default();
        let sb = SliderBox::new::<AudioMessage>(0, tx, uu);
        temp.pwin.append_property("File target", &file);
        temp.pwin.props_box.pack_start(&sb.cont, false, true, 5);
        let mut ctx = AudioUI { file, temp, params, cnf, sb };
        ctx.bind();
        ctx
    }
    fn bind(&mut self) {
        self.temp.bind::<AudioMessage>();
        let uu = self.temp.uu;
        let ref tx = self.temp.tx;
        use self::ActionMessageInner::Audio;
        use self::AudioMessage::*;
        self.file.connect_file_set(clone!(tx; |fb| {
            tx.send_internal((uu, Audio(ApplyButton)));
        }));
    }
    fn on_new_parameters(&mut self, p: &AudioParams) {
        println!("{:#?}", p);
        if let Some(ref uri) = p.url {
            self.file.set_uri(uri);
        }
        else {
            self.file.unselect_all();
        }
        self.params = p.clone();
        if p.chans.len() != self.sb.n_sliders() {
            self.sb.cont.destroy();
            self.sb = SliderBox::new::<AudioMessage>(p.chans.len(), self.temp.tx.clone(), self.temp.uu);
            self.temp.pwin.props_box.pack_start(&self.sb.cont, false, true, 5);
            self.temp.pwin.props_box.show_all();
        }
        let details = p.chans.iter()
            .map(|ch| {
                let idx = if let Some(patch) = ch.patch {
                    println!("patch: {}, defs: {:?}", patch, self.cnf.defs);
                    self.cnf.defs.iter().position(|&p| p == patch).map(|x| x+1).unwrap_or(0)
                } else { 0 };
                SliderDetail { vol: ch.vol as f64, patch: idx }
            })
            .collect::<Vec<_>>();
        self.sb.update_values(details);
    }
}
impl ActionUI for AudioUI {
    fn on_update(&mut self, p: &OpaqueAction) {
        self.temp.on_update(p);
        let ActionParameters::Audio(ref pars) = p.params;
        self.on_new_parameters(pars);

        if let PlaybackState::Unverified(ref errs) = p.state {
            for err in errs {
                if err.name == "url" {
                }
            }
        }
    }
    fn on_message(&mut self, m: ActionMessageInner) {
        if let ActionMessageInner::Audio(m) = m {
            use self::AudioMessage::*;
            match m {
                x @ ApplyButton | x @ OkButton => {
                    let url = self.file.get_uri();
                    let chans = self.params.chans.clone();
                    let params = AudioParams { url, chans };
                    self.temp.tx.send(Command::UpdateActionParams {
                        uuid: self.temp.uu,
                        params: ActionParameters::Audio(params)
                    });
                    if let OkButton = x {
                        self.temp.pwin.window.hide();
                    }
                },
                VolChanged(ch, val) => {
                    if self.params.chans.get(ch).is_some() {
                        let mut chans = self.params.chans.clone();
                        let url = self.params.url.clone();
                        chans[ch].vol = val;
                        let params = AudioParams { url, chans };
                        self.temp.tx.send(Command::UpdateActionParams {
                            uuid: self.temp.uu,
                            params: ActionParameters::Audio(params)
                        });
                    }
                },
                PatchChanged(ch, patch) => {
                    if patch == 0 { return }
                    println!("setting patch: {}, defs: {:?}", patch, self.cnf.defs);
                    let patch = match self.cnf.defs.get(patch-1) {
                        Some(&u) => u,
                        None => return
                    };
                    if self.params.chans.get(ch).is_some() {
                        let mut chans = self.params.chans.clone();
                        let url = self.params.url.clone();
                        chans[ch].patch = Some(patch);
                        let params = AudioParams { url, chans };
                        println!("updating patch for chan: {:?}", params);
                        self.temp.tx.send(Command::UpdateActionParams {
                            uuid: self.temp.uu,
                            params: ActionParameters::Audio(params)
                        });
                    }
                },
                CancelButton => {
                    let pc = self.params.clone();
                    self.on_new_parameters(&pc);
                    self.temp.pwin.window.hide();
                }
            }
        }

    }
    fn edit_separately(&mut self) {
        self.temp.edit_separately();
    }
    fn on_mixer(&mut self, cnf: &MixerConf) {
        self.cnf = cnf.clone();
        println!("new mixer conf: {:?}", self.cnf);
    }
    fn get_container(&mut self) -> Option<Widget> {
        self.temp.get_container()
    }
}
