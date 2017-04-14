use gtk::Adjustment;

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
    pub fn register_handler<F: Fn() + 'static>(&self, mut func: F) {
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
            let $i = $b.get_object(concat!("sqa-", stringify!($o), "-", stringify!($i)))
                .expect("Incorrect UI description, tried to get nonexistent path");
        )*
            $o { $($i),* }
    }};
    ($o:ident using $b:ident with $($f:ident),* get $($i:ident),*) => {{
        $(
            let $i = $b.get_object(concat!("sqa-", stringify!($o), "-", stringify!($i)))
                .expect("Incorrect UI description, tried to get nonexistent path");
        )*
            $o { $($i),* $(,$f)* }
    }}
}
macro_rules! clone {
    ($($n:ident),+; || $body:block) => (
        {
            $( let $n = $n.clone(); )+
                move || { $body }
        }
    );
    ($($n:ident),+; |$($p:ident),+| $body:block) => (
        {
            $( let $n = $n.clone(); )+
                move |$($p),+| { $body }
        }
    );
}