use gtk::Adjustment;
use gtk::prelude::*;

pub static INTERFACE_SRC: &str = include_str!("ui.glade");
#[derive(Clone)]
/// An object for cross-thread notification.
pub struct ThreadNotifier {
    adj: Adjustment
}
impl ThreadNotifier {
    pub fn new() -> Self {
        ThreadNotifier {
            adj: Adjustment::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
        }
    }
    pub fn notify(&self) {
        let selfish = self.clone();
        ::glib::timeout_add(0, move || {
            selfish.adj.changed();
            ::glib::Continue(false)
        });
    }
    pub fn register_handler<F: Fn() + 'static>(&self, func: F) {
        self.adj.connect_changed(move |_| {
            func()
        });
    }
}
/// I'm pretty sure this is safe. Maybe.
///
/// Seriously: glib::timeout_add() runs its handler _in the main thread_,
/// so we should be fine.
unsafe impl Send for ThreadNotifier {}

macro_rules! build {
    ($o:ident using $b:ident get $($i:ident),*) => {{
        $(
            let path = concat!("sqa-", stringify!($o), "-", stringify!($i));
            let $i = $b.get_object(path)
                .expect(&format!("Incorrect UI description, tried to get nonexistent path {}", path));
        )*
            $o { $($i),* }
    }};
    ($o:ident using $b:ident with $($f:ident),* get $($i:ident),*) => {{
        $(
            let path = concat!("sqa-", stringify!($o), "-", stringify!($i));
            let $i = $b.get_object(path)
                .expect(&format!("Incorrect UI description, tried to get nonexistent path {}", path));
        )*
            $o { $($i),* $(,$f)* }
    }};
    ($o:ident using $b:ident with $($f:ident),* default $($d:ident),* get $($i:ident),*) => {{
        $(
            let $d = Default::default();
        )*
            build!($o using $b with $($f),* $(,$d)* get $($i),*)
    }};
    ($o:ident using $b:ident default $($d:ident),* get $($i:ident),*) => {{
        $(
            let $d = Default::default();
        )*
            build!($o using $b with $($d),* get $($i),*)
    }}
}
macro_rules! bind_menu_items {
    ($self:ident, $tx:ident, $($name:ident => $res:expr),*) => {
        $(
            $self.$name.connect_activate(clone!($tx; |_| {
                    $tx.send_internal($res);
            }));
        )*
    }
}
macro_rules! message_impls {
    ($msg:ident, $($variant:ident, $ty:ty),*) => {
        $(
            impl From<$ty> for $msg {
                fn from(obj: $ty) -> $msg {
                    $msg::$variant(obj)
                }
            }
        )*
    }
}
macro_rules! action_reply_notify {
    ($self:ident, $res:ident, $failmsg:expr, $successmsg:expr) => {{
        let msg;
        let mut ok = true;
        if let Err(e) = $res {
            msg = Message::Error(format!(concat!($failmsg, " failed: {}"), e));
            ok = false;
        }
        else {
            msg = Message::Statusbar($successmsg.into());
        }
        $self.tx.as_mut().unwrap().send_internal(msg);
        ok
    }}
}
macro_rules! clone {
    (@param _) => ( _ );
    (@param $x:ident) => ( $x );
    ($($n:ident),+; || $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move || $body
        }
    );
    ($($n:ident),+; |$($p:tt),+| $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move |$(clone!(@param $p),)+| $body
        }
    );
}
