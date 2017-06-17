use gtk::prelude::*;
use gtk::{FileChooserAction, FileChooserButton, Widget};
use super::{ActionUI, OpaqueAction, UITemplate, ActionMessageInner};
use sync::UISender;
use uuid::Uuid;
use widgets::{SliderBox, Patched, PatchedSliderMessage, SliderMessage, SliderDetail};
use sqa_backend::codec::Command;
use sqa_backend::mixer::MixerConf;
use sqa_backend::actions::{ActionParameters, PlaybackState};
use sqa_backend::actions::audio::AudioParams;

#[derive(Clone)]
pub enum AudioMessage {
    Slider(usize, PatchedSliderMessage),
    FileChanged
}
impl SliderMessage<Patched> for AudioMessage {
    type Message = (Uuid, ActionMessageInner);
    type Identifier = Uuid;

    fn on_payload(ch: usize, data: PatchedSliderMessage, id: Uuid) -> Self::Message {
        use self::ActionMessageInner::*;
        use self::AudioMessage::*;
        (id, Audio(Slider(ch, data)))
    }
}
pub struct AudioUI {
    file: FileChooserButton,
    params: AudioParams,
    temp: UITemplate,
    cnf: MixerConf,
    sb: SliderBox<Patched, AudioMessage>
}

impl AudioUI {
    pub fn new(uu: Uuid, tx: UISender) -> Self {
        let file = FileChooserButton::new("Audio file", FileChooserAction::Open);
        let mut temp = UITemplate::new(uu, tx.clone());
        let params = Default::default();
        let cnf = Default::default();
        let sb = SliderBox::new(0, 0, &tx, uu);
        temp.get_tab("Basics").append_property("File target", &file);
        let patch = temp.add_tab("Levels &amp; Patch");
        patch.container.pack_start(&sb.grid, false, true, 5);
        let mut ctx = AudioUI { file, temp, params, cnf, sb };
        ctx.bind();
        ctx
    }
    fn bind(&mut self) {
        self.temp.bind();
        let uu = self.temp.uu;
        let ref tx = self.temp.tx;
        self.file.connect_file_set(clone!(tx; |_| {
            tx.send_internal((uu, ActionMessageInner::Audio(AudioMessage::FileChanged)));
        }));
    }
    fn on_new_parameters(&mut self, p: &AudioParams) {
        trace!("audio: new parameters {:?}", p);
        if let Some(ref uri) = p.url {
            self.file.set_uri(uri);
        }
        else {
            self.file.unselect_all();
        }
        self.params = p.clone();
        if p.chans.len() != self.sb.n_sliders() || self.cnf.defs.len() != self.sb.n_output() {
            trace!("audio: recreating sliders!");
            self.sb.grid.destroy();
            self.sb = SliderBox::new(p.chans.len(), self.cnf.defs.len(), &self.temp.tx, self.temp.uu);
            let tab = self.temp.get_tab("Levels &amp; Patch");
            tab.container.pack_start(&self.sb.grid, false, true, 5);
            tab.container.show_all();
        }
        let mut details = p.chans.iter()
            .map(|ch| {
                let idx = if let Some(patch) = ch.patch {
                    self.cnf.defs.iter().position(|&p| p == patch).map(|x| x+1).unwrap_or(0)
                } else { 0 };
                SliderDetail { vol: ch.vol as f64, patch: idx }
            })
            .collect::<Vec<_>>();
        details.insert(0, SliderDetail { vol: p.master_vol as f64, patch: 0 });
        self.sb.update_values(details);
    }
    fn apply_changes(&mut self) {
        trace!("audio: sending update {:?}", self.params);
        self.temp.tx.send(Command::UpdateActionParams {
            uuid: self.temp.uu,
            params: ActionParameters::Audio(self.params.clone())
        });
    }
}
impl ActionUI for AudioUI {
    fn on_update(&mut self, p: &OpaqueAction) {
        self.temp.on_update(p);
        if let ActionParameters::Audio(ref pars) = p.params {
            self.on_new_parameters(pars);

            if let PlaybackState::Unverified(ref errs) = p.state {
                for err in errs {
                    if err.name == "url" {
                    }
                }
            }
        }
    }
    fn close_window(&mut self) {
        self.temp.pwin.window.hide();
    }
    fn on_message(&mut self, m: ActionMessageInner) {
        if let ActionMessageInner::Audio(m) = m {
            use self::AudioMessage::*;
            match m {
                FileChanged => {
                    self.params.url = self.file.get_uri();
                    self.apply_changes();
                },
                Slider(ch, PatchedSliderMessage::VolChanged(val)) => {
                    trace!("audio: slider, ch {} val {}", ch, val);
                    if ch == 0 {
                        self.params.master_vol = val;
                    }
                    else {
                        if self.params.chans.get(ch-1).is_some() {
                            self.params.chans[ch-1].vol = val;
                        }
                    }
                    self.apply_changes();
                },
                Slider(ch, PatchedSliderMessage::PatchChanged(patch)) => {
                    if patch == 0 || ch == 0 { return }
                    let ch = ch - 1;
                    trace!("audio: setting patch for {}: {}, defs: {:?}", ch, patch, self.cnf.defs);
                    let patch = match self.cnf.defs.get(patch-1) {
                        Some(&u) => u,
                        None => return
                    };
                    if self.params.chans.get(ch).is_some() {
                        self.params.chans[ch].patch = Some(patch);
                        self.apply_changes();
                    }
                }
            }
        }

    }
    fn edit_separately(&mut self) {
        self.temp.edit_separately();
    }
    fn on_mixer(&mut self, cnf: &MixerConf) {
        self.cnf = cnf.clone();
        let p = self.params.clone();
        self.on_new_parameters(&p);
    }
    fn get_container(&mut self) -> Option<Widget> {
        self.temp.get_container()
    }
    fn change_cur_page(&mut self, cp: Option<u32>) {
        self.temp.change_cur_page(cp)
    }
}
