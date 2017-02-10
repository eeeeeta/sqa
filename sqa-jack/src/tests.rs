//! Tests
use *;
use std::thread;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::*;
use std::sync::Arc;

#[test]
fn logging_handler() {
    let atomic = Arc::new(AtomicBool::new(false));
    struct Special(Arc<AtomicBool>);
    impl JackLoggingHandler for Special {
        fn on_error(&mut self, s: &str) {
            println!("JACK ERROR: {}", s);
            self.0.store(true, Relaxed)
        }
        fn on_info(&mut self, s: &str) {
            println!("JACK INFO: {}", s);
            self.0.store(true, Relaxed)
        }
    }
    fn run() -> JackResult<JackNFrames> {
        let conn = JackConnection::connect("Testing", None)?;
        Ok(conn.sample_rate()) // do something with conn to avoid optimizer
    }
    set_logging_handler(Special(atomic.clone()));
    run().unwrap();
    assert_eq!(atomic.load(Relaxed), true);
}
#[test]
fn logging_handler_update() {
    ::std::thread::sleep(::std::time::Duration::new(2, 0));
    logging_handler();
}
#[test]
fn logging_handler_panicking() {
    ::std::thread::sleep(::std::time::Duration::new(2, 0));
    let atomic = Arc::new(AtomicBool::new(false));
    struct Special(Arc<AtomicBool>);
    impl JackLoggingHandler for Special {
        // new logging handler: panics on every message!
        fn on_error(&mut self, s: &str) {
            self.0.store(true, Relaxed);
            panic!("JACK ERROR: {}", s);
        }
        fn on_info(&mut self, s: &str) {
            self.0.store(true, Relaxed);
            panic!("JACK INFO: {}", s);
        }
    }
    fn run() -> JackResult<JackNFrames> {
        let conn = JackConnection::connect("Testing", None)?;
        Ok(conn.sample_rate()) // do something with conn to avoid optimizer
    }
    set_logging_handler(Special(atomic.clone()));
    run().unwrap();
    assert_eq!(atomic.load(Relaxed), true);
}
#[test]
fn panicking() {
    let i_witnessed_a_panic = Arc::new(AtomicBool::new(false));
    fn run(iwp: Arc<AtomicBool>) -> JackResult<()> {
        let mut conn = JackConnection::connect("Testing", None)?;
        let mut first = false;
        conn.set_handler(move |_: &JackCallbackContext| {
            if !first {
                first = true;
            }
            else {
                // uh-oh, JACK called us again after panicking!
                iwp.store(true, Relaxed);
            }
            panic!("Hi, I'm a closure, and I'm a panic-a-holic.");
        })?;
        let _ = match conn.activate() {
            Ok(nc) => nc,
            Err((_, err)) => return Err(err)
        };
        thread::sleep(::std::time::Duration::new(2, 0));
        Ok(())
    }
    run(i_witnessed_a_panic.clone()).unwrap();
    assert_eq!(i_witnessed_a_panic.load(Relaxed), false);
}
#[test]
fn thread_init() {
    let atomic = Arc::new(AtomicBool::new(false));
    struct Special(Arc<AtomicBool>);
    impl JackHandler for Special {
        fn thread_init(&mut self) {
            self.0.store(true, Relaxed)
        }
    }
    fn run(atomic: Arc<AtomicBool>) -> JackResult<()> {
        let mut conn = JackConnection::connect("Testing", None)?;
        let special = Special(atomic);
        conn.set_handler(special)?;
        let _ = match conn.activate() {
            Ok(nc) => nc,
            Err((_, err)) => return Err(err)
        };
        thread::sleep(::std::time::Duration::new(2, 0));
        Ok(())
    }
    run(atomic.clone()).unwrap();
    assert_eq!(atomic.load(Relaxed), true);
}
#[test]
fn sawtooth() {
    struct Sawtooth {
        out1: JackPort,
        out2: JackPort,
        left_saw: f32,
        right_saw: f32,
        xrun: Arc<AtomicBool>
    }
    impl JackHandler for Sawtooth {
        fn process(&mut self, ctx: &JackCallbackContext) -> JackControl {
            let out1 = ctx.get_port_buffer(&self.out1).unwrap();
            let out2 = ctx.get_port_buffer(&self.out2).unwrap();
            for (out1, out2) in out1.iter_mut().zip(out2.iter_mut()) {
                *out1 = self.left_saw * 0.1;
                *out2 = self.right_saw * 0.1;
                self.left_saw += 0.01;
                if self.left_saw >= 1.0 { self.left_saw -= 2.0; }
                self.right_saw += 0.03;
                if self.right_saw >= 1.0 { self.right_saw -= 2.0; }
            }
            JackControl::Continue
        }
        #[cfg(feature="test-xruns")]
        fn xrun(&mut self) -> JackControl {
            self.xrun.store(true, Relaxed);
            JackControl::Continue
        }
    }
    fn run(b: Arc<AtomicBool>) -> JackResult<()> {
        let mut conn = JackConnection::connect("Very Annoying Sawtooth Generator", None)?;
        let out1 = conn.register_port("output_1", PORT_IS_OUTPUT)?;
        let out2 = conn.register_port("output_2", PORT_IS_OUTPUT)?;
        let data = Sawtooth {
            out1: out1,
            out2: out2,
            left_saw: 0.0,
            right_saw: 0.0,
            xrun: b
        };
        conn.set_handler(data)?;
        let mut conn = match conn.activate() {
            Ok(nc) => nc,
            Err((_, err)) => return Err(err)
        };
        let ports = conn.get_ports(None, None, Some(PORT_IS_INPUT | PORT_IS_PHYSICAL))?;
        if ports.len() >= 2 {
            conn.connect_ports(&out1, &ports[0])?;
            conn.connect_ports(&out2, &ports[1])?;
        }
        thread::sleep(::std::time::Duration::new(2, 0));
        Ok(())
    }
    let atomic = Arc::new(AtomicBool::new(false));
    run(atomic.clone()).unwrap();
    assert_eq!(atomic.load(Relaxed), false);
}
