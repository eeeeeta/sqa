//! Module for keeping track of the SQA Engine.
use uuid::Uuid;
use sqa_engine::{EngineContext, BufferSender, sqa_jack};
use std::collections::HashMap;
use errors::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Channel {
    pub name: String,
    pub uuid: Uuid,
    pub eid: usize,
    pub patch: Option<String>
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MixerConf {
    channels: Vec<Channel>,
    defs: Vec<Uuid>
}
pub struct MixerContext {
    engine: EngineContext,
    channels: HashMap<Uuid, Channel>,
    defs: Vec<Uuid>
}

impl MixerContext {
    pub fn new() -> BackendResult<Self> {
        let ec = EngineContext::new(Some("sqa-backend"))?;
        Ok(MixerContext {
            engine: ec,
            channels: HashMap::new(),
            defs: vec![]
        })
    }
    pub fn default_config(&mut self) -> BackendResult<()> {
        for (i, port) in self.engine.conn
            .get_ports(None, None, Some(sqa_jack::PORT_IS_INPUT | sqa_jack::PORT_IS_PHYSICAL))?
            .into_iter()
            .enumerate() {
                let name = format!("default-chan-{}", i);
                let eid = self.engine.new_channel(&name)?;
                let patch = port.get_name(false)?;
                let uu = Uuid::new_v4();
                self.engine.conn.connect_ports(self.engine.chans[eid].as_ref().unwrap(), &port)?;
                self.channels.insert(uu, Channel {
                    name: name,
                    uuid: uu,
                    eid: eid,
                    patch: Some(patch.into())
                });
                self.defs.push(uu);
            }
        Ok(())
    }
    pub fn obtain_config(&mut self) -> MixerConf {
        let mut ret = vec![];
        for (_, ch) in self.channels.iter_mut() {
            ret.push(ch.clone());
        }
        MixerConf {
            channels: ret,
            defs: self.defs.clone()
        }
    }
    pub fn obtain_def(&self, idx: usize) -> Option<Uuid> {
        self.defs.get(idx).map(|x| *x)
    }
    pub fn new_sender(&mut self, sample_rate: u64) -> BufferSender {
        self.engine.new_sender(sample_rate)
    }
    pub fn obtain_channel(&self, uu: &Uuid) -> Option<usize> {
        self.channels.get(uu).map(|x| x.eid)
    }
    pub fn process_config(&mut self, conf: MixerConf) -> BackendResult<()> {
        let mut touched = vec![];
        for mut ch in conf.channels {
            /* The following weird structure is brought to you by the borrow checker */
            let x = if let Some(ref mut c2) = self.channels.get_mut(&ch.uuid) {
                let mut ech = self.engine.chans[c2.eid].ok_or("Channel removed or logic error")?;
                if ch.name != c2.name {
                    ech.set_short_name(&ch.name)?;
                    c2.name = ch.name;
                }
                if ch.patch != c2.patch {
                    if let Some(ref old) = c2.patch {
                        if let Ok(port) = self.engine.conn.get_port_by_name(&old) {
                            let _ = /* We don't care if we can't disconnect the port: it may have gone
                                away or something, and throwing an error here is unhelpful */
                                self.engine.conn.disconnect_ports(&ech, &port);
                        }
                    }
                    if let Some(new) = ch.patch {
                        let port = self.engine.conn.get_port_by_name(&new)?;
                        self.engine.conn.connect_ports(&ech, &port)?;
                    }
                }
                touched.push(c2.uuid);
                None
            }
            else {
                ch.eid = self.engine.new_channel(&ch.name)?;
                if let Some(ref new) = ch.patch {
                    let port = self.engine.conn.get_port_by_name(&new)?;
                    self.engine.conn.connect_ports(self.engine.chans[ch.eid].as_ref().unwrap(), &port)?;
                }
                touched.push(ch.uuid);
                Some(ch)
            };
            if let Some(ch) = x {
                /* We can't use self.channels in the else block above
                   because *borrowck reasons*, so we do it here */
                self.channels.insert(ch.uuid.clone(), ch);
            }
        }
        self.defs = conf.defs;
        self.defs.retain(|uu| {
            touched.contains(uu)
        });
        for (uu, ch) in self.channels.iter_mut() {
            if !touched.contains(uu) {
                self.engine.remove_channel(ch.eid)?;
            }
        }
        self.channels.retain(|uu, _| {
            touched.contains(uu)
        });
        Ok(())
    }
}
